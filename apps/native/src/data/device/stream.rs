//! The device helper's stream, as serve-sim frames it: a u32 big-endian
//! length (of the tag and payload), a tag byte, the payload. Read in pieces
//! as they arrive from the socket; and the avcC record a decoder needs, cut
//! into its SPS and PPS.

use serde_json::Value;

/// Past this a chunk is not the helper's (a 4K screen's keyframe is ~2 MB).
const MAX_CHUNK: usize = 64 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub enum Chunk {
    /// The avcC record: the decoder starts over with it.
    Description(Vec<u8>),
    /// An access unit in AVCC framing (4-byte NAL lengths).
    Frame { keyframe: bool, data: Vec<u8> },
    /// The answer to a command (`{"id", "ok", "result" | "error"}`).
    Reply(Value),
    /// `{"started": true}`, `{"width", "height"}`.
    Status(Value),
}

/// Chunks out of the bytes as they come.
#[derive(Default)]
pub struct Reader {
    pending: Vec<u8>,
}

impl Reader {
    /// The chunks the bytes so far complete; `Err` when the stream is not
    /// the helper's.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Chunk>, String> {
        self.pending.extend_from_slice(bytes);
        let mut chunks = Vec::new();
        let mut at = 0;
        while self.pending.len() - at >= 4 {
            let length = u32::from_be_bytes(self.pending[at..at + 4].try_into().unwrap()) as usize;
            if length == 0 || length > MAX_CHUNK {
                return Err(format!("bad chunk length {length}"));
            }
            if self.pending.len() - at - 4 < length {
                break;
            }
            let tag = self.pending[at + 4];
            let payload = self.pending[at + 5..at + 4 + length].to_vec();
            at += 4 + length;
            let json = || serde_json::from_slice::<Value>(&payload).ok();
            match tag {
                0x01 => chunks.push(Chunk::Description(payload)),
                0x02 | 0x03 => chunks.push(Chunk::Frame {
                    keyframe: tag == 0x02,
                    data: payload,
                }),
                0x05 => chunks.extend(json().map(Chunk::Reply)),
                0x06 => chunks.extend(json().map(Chunk::Status)),
                // A JPEG seed or anything newer: not ours to draw.
                _ => {}
            }
        }
        self.pending.drain(..at);
        Ok(chunks)
    }
}

/// The parameter sets in an avcC record and its NAL length size.
#[derive(Debug, PartialEq)]
pub struct ParameterSets {
    pub sets: Vec<Vec<u8>>,
    pub nal_length: usize,
}

/// ISO/IEC 14496-15 `AVCDecoderConfigurationRecord`.
pub fn parameter_sets(avcc: &[u8]) -> Option<ParameterSets> {
    if avcc.len() < 7 || avcc[0] != 1 {
        return None;
    }
    let nal_length = (avcc[4] & 0b11) as usize + 1;
    let mut at = 5;
    let mut sets = Vec::new();
    for count_mask in [0b1_1111u8, 0xFF] {
        let count = (*avcc.get(at)? & count_mask) as usize;
        at += 1;
        for _ in 0..count {
            let length = u16::from_be_bytes([*avcc.get(at)?, *avcc.get(at + 1)?]) as usize;
            at += 2;
            sets.push(avcc.get(at..at + length)?.to_vec());
            at += length;
        }
    }
    (sets.len() >= 2).then_some(ParameterSets { sets, nal_length })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 1) as u32).to_be_bytes().to_vec();
        out.push(tag);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn chunks_come_whole_however_the_bytes_are_cut() {
        let mut bytes = chunk(0x06, br#"{"width":1206,"height":2622}"#);
        bytes.extend(chunk(0x01, &[1, 2]));
        bytes.extend(chunk(0x02, &[0, 0, 0, 1, 0x65]));
        bytes.extend(chunk(0x04, b"jpeg"));
        bytes.extend(chunk(0x03, &[0, 0, 0, 1, 0x41]));
        let mut reader = Reader::default();
        let mut chunks = Vec::new();
        for piece in bytes.chunks(3) {
            chunks.extend(reader.push(piece).unwrap());
        }
        assert_eq!(chunks.len(), 4, "the seed is skipped");
        assert_eq!(
            chunks[0],
            Chunk::Status(serde_json::json!({"width": 1206, "height": 2622}))
        );
        assert_eq!(chunks[1], Chunk::Description(vec![1, 2]));
        assert!(matches!(&chunks[2], Chunk::Frame { keyframe: true, .. }));
        assert!(matches!(
            &chunks[3],
            Chunk::Frame {
                keyframe: false,
                ..
            }
        ));
        assert!(Reader::default().push(&[0, 0, 0, 0, 1]).is_err());
    }

    #[test]
    fn an_avcc_record_gives_its_sps_and_pps() {
        let avcc = [
            1, 0x64, 0, 0x1f, 0xFF, // version, profile, compat, level, 4-byte NAL lengths
            0xE1, 0, 3, 0x67, 0xAA, 0xBB, // one SPS
            1, 0, 2, 0x68, 0xCC, // one PPS
        ];
        let sets = parameter_sets(&avcc).unwrap();
        assert_eq!(sets.nal_length, 4);
        assert_eq!(sets.sets, vec![vec![0x67, 0xAA, 0xBB], vec![0x68, 0xCC]]);
        assert_eq!(parameter_sets(&avcc[..9]), None, "cut short");
        assert_eq!(parameter_sets(&[0; 8]), None);
    }
}
