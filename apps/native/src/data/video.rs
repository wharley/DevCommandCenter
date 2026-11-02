//! An attached video's poster and length, for its card in the person's
//! message. The poster is the first frame as the system's Quick Look draws
//! it (`qlmanage`; `ffmpeg` elsewhere, when it is there), kept in the temp
//! folder; the length comes from the file's own `mvhd` (MP4, MOV, M4V).

use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The poster's longest side, twice the card's for a sharp picture.
const POSTER_SIDE: u32 = 448;

#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    pub poster: PathBuf,
    /// The poster's size in pixels.
    pub size: (f32, f32),
    pub seconds: Option<f64>,
}

/// The poster and length of the video at `path`, or `None` without a poster.
pub fn read(path: &Path) -> Option<Video> {
    let poster = poster(path)?;
    let (width, height) = image::image_dimensions(&poster).ok()?;
    (width > 0 && height > 0).then(|| Video {
        poster,
        size: (width as f32, height as f32),
        seconds: duration(path),
    })
}

fn poster(path: &Path) -> Option<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .hash(&mut hasher);
    let folder = std::env::temp_dir()
        .join("dcc-video-posters")
        .join(format!("{:016x}", hasher.finish()));
    let name = format!("{}.png", path.file_name()?.to_string_lossy());
    let poster = folder.join(&name);
    if poster.is_file() {
        return Some(poster);
    }
    std::fs::create_dir_all(&folder).ok()?;
    let made = if cfg!(target_os = "macos") {
        Command::new("qlmanage")
            .args(["-t", "-s", &POSTER_SIDE.to_string(), "-o"])
            .arg(&folder)
            .arg(path)
            .output()
    } else {
        Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &format!("scale={POSTER_SIDE}:{POSTER_SIDE}:force_original_aspect_ratio=decrease"),
            ])
            .arg(&poster)
            .output()
    };
    (made.is_ok_and(|output| output.status.success()) && poster.is_file()).then_some(poster)
}

/// The movie header's length: top-level boxes are walked by their sizes,
/// so a `moov` at the end of a large file is found without reading it all.
pub fn duration(path: &Path) -> Option<f64> {
    let mut file = File::open(path).ok()?;
    let end = file.metadata().ok()?.len();
    let (moov, moov_end) = find_box(&mut file, 0, end, b"moov")?;
    let (mvhd, _) = find_box(&mut file, moov, moov_end, b"mvhd")?;
    file.seek(SeekFrom::Start(mvhd)).ok()?;
    let mut header = [0u8; 32];
    file.read_exact(&mut header).ok()?;
    let (scale, length) = if header[0] == 1 {
        (
            u32::from_be_bytes(header[20..24].try_into().ok()?) as f64,
            u64::from_be_bytes(header[24..32].try_into().ok()?) as f64,
        )
    } else {
        (
            u32::from_be_bytes(header[12..16].try_into().ok()?) as f64,
            u32::from_be_bytes(header[16..20].try_into().ok()?) as f64,
        )
    };
    (scale > 0.0).then(|| length / scale)
}

/// The body of the first `kind` box between `from` and `to`: its start and end.
fn find_box(file: &mut File, mut from: u64, to: u64, kind: &[u8; 4]) -> Option<(u64, u64)> {
    while from + 8 <= to {
        file.seek(SeekFrom::Start(from)).ok()?;
        let mut head = [0u8; 8];
        file.read_exact(&mut head).ok()?;
        let mut size = u32::from_be_bytes(head[0..4].try_into().ok()?) as u64;
        let mut body = from + 8;
        if size == 1 {
            let mut large = [0u8; 8];
            file.read_exact(&mut large).ok()?;
            size = u64::from_be_bytes(large);
            body += 8;
        } else if size == 0 {
            size = to - from;
        }
        if size < body - from {
            return None;
        }
        if &head[4..8] == kind {
            return Some((body, from + size));
        }
        from += size;
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal MP4: `ftyp`, an `mdat` and a `moov` with a version 0 `mvhd`
    /// of 81.5 seconds at 1000 a second.
    pub(crate) fn mp4(dir: &Path) -> PathBuf {
        fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }
        let mut mvhd = vec![0u8; 100];
        mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&81_500u32.to_be_bytes());
        let mut file = boxed(b"ftyp", b"isom\0\0\0\0");
        file.extend(boxed(b"mdat", &[7u8; 64]));
        file.extend(boxed(b"moov", &boxed(b"mvhd", &mvhd)));
        let path = dir.join("tela.mp4");
        std::fs::write(&path, file).unwrap();
        path
    }

    #[test]
    fn the_length_comes_from_the_movie_header_after_the_media() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(duration(&mp4(dir.path())), Some(81.5));
        let other = dir.path().join("nada.mp4");
        std::fs::write(&other, b"not a movie").unwrap();
        assert_eq!(duration(&other), None);
    }
}
