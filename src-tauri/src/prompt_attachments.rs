//! Files a person attaches to a prompt. The composer serializes each one into
//! the prompt as `@/absolute/path` (`apps/desktop/src/lib/prompt-attachments.ts`)
//! and providers with native image input attach the images it references
//! (`dcc-providers` `referenced_image_paths`). Pasted images live in the OS
//! temp dir; the phone's attachments land in the same place, the same way.
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Largest image a thumbnail is read for (the desktop's composer preview).
pub const IMAGE_PREVIEW_MAX_BYTES: u64 = 15 * 1024 * 1024;

/// Largest file the phone may attach, after its own compression.
pub const MAX_PHONE_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// Writes a pasted image as `dcc_img_<uuid>.<extension>` in the temp dir.
pub fn save_temp_image(bytes: &[u8], extension: &str) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("dcc_img_{}.{}", Uuid::new_v4(), extension));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Writes any other file under `dcc_file_<uuid>/` keeping a readable name, so
/// the agent sees `relatorio.pdf` rather than an opaque id.
pub fn save_temp_file(bytes: &[u8], name: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("dcc_file_{}", Uuid::new_v4()));
    std::fs::create_dir(&dir)?;
    let path = dir.join(safe_file_name(name));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Image kind by content, never by the name a client sent.
pub fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

/// A name that survives the `@path` serialization: one path segment, no
/// whitespace or markup, and an extension (the desktop's parser needs one).
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    cleaned = cleaned.trim_matches(['.', '-']).to_string();
    if cleaned.chars().count() > 80 {
        let (stem, ext) = split_extension(&cleaned);
        let stem: String = stem.chars().take(60).collect();
        cleaned = match ext {
            Some(ext) => format!("{stem}.{ext}"),
            None => stem,
        };
    }
    if cleaned.is_empty() {
        cleaned = "arquivo".into();
    }
    if split_extension(&cleaned).1.is_none() {
        cleaned.push_str(".txt");
    }
    cleaned
}

fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && (1..=12).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            (stem, Some(ext))
        }
        _ => (name, None),
    }
}

pub fn image_media_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        _ => None,
    }
}

/// An absolute image path as a data URL, for the thumbnail in the person's
/// message. `None` when it is not an image, too large or gone (the OS cleans
/// the temp dir): a normal outcome.
pub fn read_image_data_url(path: &Path, max_bytes: u64) -> Option<String> {
    use base64::Engine as _;
    if !path.is_absolute() {
        return None;
    }
    let media_type = image_media_type(path)?;
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{media_type};base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_stay_one_segment_with_an_extension() {
        assert_eq!(safe_file_name("relatório final.pdf"), "relatório-final.pdf");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd.txt");
        assert_eq!(safe_file_name("C:\\x\\notes.md"), "notes.md");
        assert_eq!(safe_file_name("<script>.html"), "script-.html");
        assert_eq!(safe_file_name(".env"), "env.txt");
        assert_eq!(safe_file_name(""), "arquivo.txt");
        let long = format!("{}.json", "a".repeat(200));
        let cleaned = safe_file_name(&long);
        assert!(cleaned.ends_with(".json") && cleaned.chars().count() <= 65);
    }

    #[test]
    fn images_are_recognized_by_content() {
        assert_eq!(
            sniff_image(b"\x89PNG\r\n\x1a\nrest"),
            Some(("image/png", "png"))
        );
        assert_eq!(
            sniff_image(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(("image/jpeg", "jpg"))
        );
        assert_eq!(sniff_image(b"%PDF-1.7"), None);
    }

    #[test]
    fn saved_files_are_previewable_only_when_they_are_images() {
        let image = save_temp_image(b"\x89PNG\r\n\x1a\nfixture", "png").unwrap();
        assert!(read_image_data_url(&image, IMAGE_PREVIEW_MAX_BYTES)
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(read_image_data_url(&image, 4).is_none());
        let file = save_temp_file(b"hello", "notas.md").unwrap();
        assert_eq!(file.file_name().unwrap(), "notas.md");
        assert!(read_image_data_url(&file, IMAGE_PREVIEW_MAX_BYTES).is_none());
        std::fs::remove_file(image).unwrap();
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
}
