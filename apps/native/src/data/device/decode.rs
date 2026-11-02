//! The simulator's H.264 into the picture GPUI draws: VideoToolbox decodes
//! each access unit, synchronously, into a Metal-compatible NV12
//! (`420YpCbCr8BiPlanarFullRange`) `CVPixelBuffer` — the one format GPUI's
//! `surface()` takes (it asserts on any other, so a frame in another format
//! is dropped here instead). A new avcC record starts a new session.

use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Mutex;

use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime,
    CMVideoFormatDescriptionCreateFromH264ParameterSets,
};
use objc2_core_video::{
    CVImageBuffer, CVPixelBuffer, CVPixelBufferGetPixelFormatType,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession,
};

use super::stream::parameter_sets;

pub type Picture = CFRetained<CVPixelBuffer>;

/// Where the output callback leaves the decoded picture.
type Slot = Mutex<Option<Picture>>;

pub struct Decoder {
    session: CFRetained<VTDecompressionSession>,
    format: CFRetained<CMFormatDescription>,
    /// Boxed so its address, given to VideoToolbox, never moves.
    slot: Box<Slot>,
}

unsafe extern "C-unwind" fn decoded(
    refcon: *mut c_void,
    _frame: *mut c_void,
    status: i32,
    _flags: VTDecodeInfoFlags,
    image: *mut CVImageBuffer,
    _pts: CMTime,
    _duration: CMTime,
) {
    let Some(image) = NonNull::new(image) else {
        return;
    };
    if status != 0 || refcon.is_null() {
        return;
    }
    // SAFETY: `refcon` is the decoder's boxed slot, alive for the session.
    let slot = unsafe { &*(refcon as *const Slot) };
    // SAFETY: VideoToolbox lends the image for the callback; retain it.
    let picture = unsafe { CFRetained::retain(image) };
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(picture);
    }
}

/// `{ PixelFormatType: '420f', MetalCompatibility: true, IOSurfaceProperties: {} }`.
fn destination_attributes() -> CFRetained<CFDictionary<CFString, CFType>> {
    let format = CFNumber::new_i32(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange as i32);
    let metal = CFBoolean::new(true);
    let surface = CFDictionary::<CFString, CFType>::empty();
    // SAFETY: CoreVideo's key constants are valid for the process.
    let keys: [&CFString; 3] = unsafe {
        [
            kCVPixelBufferPixelFormatTypeKey,
            kCVPixelBufferMetalCompatibilityKey,
            kCVPixelBufferIOSurfacePropertiesKey,
        ]
    };
    CFDictionary::from_slices(&keys, &[&**format, &**metal, &**surface])
}

impl Decoder {
    /// A session for the stream an avcC record describes.
    pub fn new(avcc: &[u8]) -> Result<Self, String> {
        let sets = parameter_sets(avcc).ok_or("not an avcC record")?;
        let pointers: Vec<NonNull<u8>> = sets
            .sets
            .iter()
            .map(|set| NonNull::new(set.as_ptr() as *mut u8).expect("a parameter set"))
            .collect();
        let sizes: Vec<usize> = sets.sets.iter().map(Vec::len).collect();
        let mut format: *const CMFormatDescription = ptr::null();
        // SAFETY: the pointers and sizes describe `sets`, alive for the call.
        let status = unsafe {
            CMVideoFormatDescriptionCreateFromH264ParameterSets(
                None,
                pointers.len(),
                NonNull::new(pointers.as_ptr() as *mut NonNull<u8>).unwrap(),
                NonNull::new(sizes.as_ptr() as *mut usize).unwrap(),
                sets.nal_length as i32,
                NonNull::from(&mut format),
            )
        };
        let format = NonNull::new(format as *mut CMFormatDescription)
            .filter(|_| status == 0)
            .ok_or(format!("format description: {status}"))?;
        // SAFETY: a +1 object from a Create function.
        let format = unsafe { CFRetained::from_raw(format) };
        let slot: Box<Slot> = Box::new(Mutex::new(None));
        let record = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(decoded),
            decompressionOutputRefCon: &*slot as *const Slot as *mut c_void,
        };
        let attributes = destination_attributes();
        let mut session: *mut VTDecompressionSession = ptr::null_mut();
        // SAFETY: `record` points at the boxed slot, which lives as long as
        // the session (it is dropped after the session is invalidated).
        let status = unsafe {
            VTDecompressionSession::create(
                None,
                &format,
                None,
                Some(attributes.as_opaque()),
                &record,
                NonNull::from(&mut session),
            )
        };
        let session = NonNull::new(session)
            .filter(|_| status == 0)
            .ok_or(format!("decompression session: {status}"))?;
        Ok(Self {
            // SAFETY: a +1 object from a Create function.
            session: unsafe { CFRetained::from_raw(session) },
            format,
            slot,
        })
    }

    /// One access unit (AVCC framing) into its picture; `None` while the
    /// decoder needs more (a delta before any keyframe) or on a bad frame.
    pub fn decode(&mut self, data: &[u8]) -> Option<Picture> {
        if data.is_empty() {
            return None;
        }
        let mut block: *mut CMBlockBuffer = ptr::null_mut();
        // SAFETY: CoreMedia allocates the block (memory null, default
        // allocator); the bytes are copied in right after.
        let status = unsafe {
            CMBlockBuffer::create_with_memory_block(
                None,
                ptr::null_mut(),
                data.len(),
                None,
                ptr::null(),
                0,
                data.len(),
                objc2_core_media::kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut block),
            )
        };
        let block = NonNull::new(block).filter(|_| status == 0)?;
        // SAFETY: a +1 object from a Create function.
        let block = unsafe { CFRetained::from_raw(block) };
        // SAFETY: `data` is valid for its length; the block holds as many bytes.
        let status = unsafe {
            CMBlockBuffer::replace_data_bytes(
                NonNull::new(data.as_ptr() as *mut c_void)?,
                &block,
                0,
                data.len(),
            )
        };
        if status != 0 {
            return None;
        }
        let sizes = [data.len()];
        let mut sample: *mut CMSampleBuffer = ptr::null_mut();
        // SAFETY: one sample with no timing, of `data.len()` bytes.
        let status = unsafe {
            CMSampleBuffer::create_ready(
                None,
                Some(&block),
                Some(&self.format),
                1,
                0,
                ptr::null(),
                1,
                sizes.as_ptr(),
                NonNull::from(&mut sample),
            )
        };
        let sample = NonNull::new(sample).filter(|_| status == 0)?;
        // SAFETY: a +1 object from a Create function.
        let sample = unsafe { CFRetained::from_raw(sample) };
        // Without the asynchronous flag the callback runs before this returns.
        // SAFETY: the session and sample are valid; no frame refcon is used.
        let status = unsafe {
            self.session.decode_frame(
                &sample,
                VTDecodeFrameFlags(0),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            return None;
        }
        let picture = self.slot.lock().ok()?.take()?;
        let format = CVPixelBufferGetPixelFormatType(&picture);
        (format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange).then_some(picture)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: no frame is in flight (decoding is synchronous); the slot
        // outlives the session's last callback.
        unsafe { self.session.invalidate() };
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_video::{CVPixelBufferGetHeight, CVPixelBufferGetWidth};

    use super::*;
    use crate::data::device::stream::{Chunk, Reader};

    /// The description, a keyframe and a delta an iPhone 16 Pro simulator sent.
    const STREAM: &[u8] = include_bytes!("testdata/iphone-16-pro.avcc");

    #[test]
    fn a_simulators_stream_decodes_to_what_gpui_draws() {
        let chunks = Reader::default().push(STREAM).unwrap();
        let Chunk::Description(avcc) = &chunks[0] else {
            panic!("description first");
        };
        let mut decoder = Decoder::new(avcc).unwrap();
        for chunk in &chunks[1..] {
            let Chunk::Frame { data, .. } = chunk else {
                panic!("frames after it");
            };
            let picture = decoder.decode(data).expect("a picture");
            assert_eq!(CVPixelBufferGetWidth(&picture), 1206);
            assert_eq!(CVPixelBufferGetHeight(&picture), 2622);
            assert_eq!(
                CVPixelBufferGetPixelFormatType(&picture),
                kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
            );
        }
        assert!(decoder.decode(&[]).is_none());
        assert!(Decoder::new(&[0, 1, 2]).is_err());
    }
}
