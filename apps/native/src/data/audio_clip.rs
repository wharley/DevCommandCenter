//! An attached audio file in the person's message (`lib/audio-attachment`):
//! its length and waveform, read once off the UI thread. CoreAudio reads the
//! formats the desktop's WebView decoded, Ogg Opus (WhatsApp voice notes)
//! included. Gone, too large or undecodable: `None`, and the message shows
//! the file chip instead.

use std::path::Path;

/// `AUDIO_WAVEFORM_BARS`.
pub const BARS: usize = 40;
/// `AUDIO_PREVIEW_MAX_BYTES`.
const MAX_BYTES: u64 = 30 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub duration: f64,
    /// Loudness of each bar, 0–1, the loudest at 1.
    pub peaks: Vec<f32>,
}

/// `audioPeaks`: RMS of `buckets` equal slices, normalized to the loudest,
/// so a voice note reads as speech and pauses rather than a flat band.
fn normalize(sums: &[(f64, usize)]) -> Vec<f32> {
    let levels: Vec<f32> = sums
        .iter()
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                (sum / *count as f64).sqrt() as f32
            }
        })
        .collect();
    let loudest = levels.iter().copied().fold(0.0_f32, f32::max);
    levels
        .into_iter()
        .map(|level| if loudest > 0.0 { level / loudest } else { 0.0 })
        .collect()
}

/// Adds `samples`, starting at frame `first`, to the slices' squared sums.
fn accumulate(sums: &mut [(f64, usize)], samples: &[f32], first: u64, total: u64) {
    let size = (total / sums.len() as u64).max(1);
    for (index, sample) in samples.iter().enumerate() {
        let bucket = (((first + index as u64) / size) as usize).min(sums.len() - 1);
        sums[bucket].0 += f64::from(*sample) * f64::from(*sample);
        sums[bucket].1 += 1;
    }
}

/// `formatAudioTime`: `0:42`, `12:05`, `1:02:09`.
pub fn format_time(seconds: f64) -> String {
    let total = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

fn readable(path: &Path) -> bool {
    path.is_absolute()
        && crate::data::prompt::is_audio_path(&path.to_string_lossy())
        && std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_BYTES)
}

/// The clip's length and waveform. Blocking: call it off the UI thread.
#[cfg(target_os = "macos")]
pub fn read(path: &Path) -> Option<Clip> {
    use objc2::AllocAnyThread;
    use objc2_avf_audio::{AVAudioFile, AVAudioPCMBuffer};
    use objc2_foundation::{NSString, NSURL};

    if !readable(path) {
        return None;
    }
    // A chunk at a time: an hour of audio never sits in memory at once.
    const CHUNK: u32 = 1 << 16;
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    // SAFETY: plain AVFAudio calls on objects this function owns; the
    // buffer's channel pointer is read only within `frameLength`.
    unsafe {
        let file = AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url).ok()?;
        let format = file.processingFormat();
        let total = u64::try_from(file.length())
            .ok()
            .filter(|frames| *frames > 0)?;
        let rate = format.sampleRate();
        let buffer = AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
            AVAudioPCMBuffer::alloc(),
            &format,
            CHUNK,
        )?;
        let mut sums = vec![(0.0, 0); BARS];
        let mut read = 0_u64;
        while read < total {
            file.readIntoBuffer_error(&buffer).ok()?;
            let frames = buffer.frameLength();
            if frames == 0 {
                break;
            }
            let channels = buffer.floatChannelData();
            if channels.is_null() {
                return None;
            }
            let first = (*channels).as_ptr();
            let samples = std::slice::from_raw_parts(first, frames as usize);
            accumulate(&mut sums, samples, read, total);
            read += u64::from(frames);
        }
        Some(Clip {
            duration: total as f64 / rate,
            peaks: normalize(&sums),
        })
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read(path: &Path) -> Option<Clip> {
    let _ = readable(path);
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A mono 16-bit WAV: a quiet first half, a loud second half.
    pub(crate) fn wav(dir: &Path) -> std::path::PathBuf {
        let rate = 8_000_u32;
        let samples: Vec<i16> = (0..rate)
            .map(|i| {
                let amplitude = if i < rate / 2 { 1_000.0 } else { 16_000.0 };
                (amplitude * (i as f32 * 0.3).sin()) as i16
            })
            .collect();
        let data_len = (samples.len() * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let path = dir.join("nota de voz.wav");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn times_read_like_a_voice_note() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(42.7), "0:42");
        assert_eq!(format_time(725.0), "12:05");
        assert_eq!(format_time(3729.0), "1:02:09");
        assert_eq!(format_time(f64::NAN), "0:00");
    }

    #[test]
    fn the_loudest_slice_is_one() {
        let mut sums = vec![(0.0, 0); 4];
        accumulate(&mut sums, &[0.1, 0.1, 0.5, 0.5], 0, 8);
        accumulate(&mut sums, &[0.0, 0.0, 0.2, 0.2], 4, 8);
        let peaks = normalize(&sums);
        assert_eq!(peaks[1], 1.0);
        assert!((peaks[0] - 0.2).abs() < 1e-6);
        assert_eq!(peaks[2], 0.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_wav_reads_its_length_and_waveform() {
        let dir = tempfile::tempdir().unwrap();
        let clip = read(&wav(dir.path())).expect("decoded");
        assert!((clip.duration - 1.0).abs() < 0.01, "{}", clip.duration);
        assert_eq!(clip.peaks.len(), BARS);
        assert!(
            clip.peaks[0] < 0.2 && clip.peaks[BARS - 1] > 0.9,
            "{:?}",
            clip.peaks
        );
        assert!(read(&dir.path().join("sumiu.opus")).is_none());
        assert!(read(Path::new("relativo.opus")).is_none());
    }
}
