use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPreview {
    kind: &'static str,
    content: Option<String>,
    data_url: Option<String>,
    reason: Option<&'static str>,
}

fn read_preview(
    workspace_root: Option<&str>,
    file_path: &str,
) -> Result<AttachmentPreview, String> {
    let supplied = Path::new(file_path);
    let path = if supplied.is_absolute() {
        supplied.canonicalize().map_err(|_| "file_unavailable")?
    } else {
        let root = Path::new(workspace_root.ok_or("workspace_required")?)
            .canonicalize()
            .map_err(|_| "file_unavailable")?;
        let path = root
            .join(supplied)
            .canonicalize()
            .map_err(|_| "file_unavailable")?;
        if !path.starts_with(&root) {
            return Err("path_outside_workspace".into());
        }
        path
    };
    if !std::fs::metadata(&path)
        .map_err(|_| "file_unavailable")?
        .is_file()
    {
        return Err("not_a_file".into());
    }
    let file = File::open(path).map_err(|_| "file_unavailable")?;
    let metadata = file.metadata().map_err(|_| "file_unavailable")?;
    if !metadata.is_file() {
        return Err("not_a_file".into());
    }
    let unavailable = |reason| AttachmentPreview {
        kind: "unavailable",
        content: None,
        data_url: None,
        reason: Some(reason),
    };
    const MAX_BYTES: u64 = 8 * 1024 * 1024;
    if metadata.len() > MAX_BYTES {
        return Ok(unavailable("tooLarge"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file_unavailable")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Ok(unavailable("tooLarge"));
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    };
    if let Some(mime) = mime {
        return Ok(AttachmentPreview {
            kind: "image",
            content: None,
            data_url: Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes))),
            reason: None,
        });
    }
    if bytes.len() > 1024 * 1024 {
        return Ok(unavailable("tooLarge"));
    }
    match String::from_utf8(bytes) {
        Ok(content) if !content.contains('\0') => Ok(AttachmentPreview {
            kind: "text",
            content: Some(content),
            data_url: None,
            reason: None,
        }),
        _ => Ok(unavailable("binary")),
    }
}

#[tauri::command]
pub async fn preview_composer_attachment(
    workspace_root: Option<String>,
    file_path: String,
) -> Result<AttachmentPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        read_preview(workspace_root.as_deref(), &file_path)
    })
    .await
    .map_err(|_| "preview_failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_workspace_text_and_explicit_external_images() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("app.ts"), "export const ready = true;").unwrap();
        let text = read_preview(root.path().to_str(), "app.ts").unwrap();
        assert_eq!(text.kind, "text");
        assert_eq!(text.content.as_deref(), Some("export const ready = true;"));
        let image = root.path().join("capture.png");
        std::fs::write(&image, b"\x89PNG\r\n\x1a\nfixture").unwrap();
        assert!(read_preview(None, image.to_str().unwrap())
            .unwrap()
            .data_url
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn rejects_relative_escape_and_directories() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(parent.path().join("outside.txt"), "outside").unwrap();
        assert_eq!(
            read_preview(root.to_str(), "../outside.txt").unwrap_err(),
            "path_outside_workspace"
        );
        assert_eq!(
            read_preview(None, root.to_str().unwrap()).unwrap_err(),
            "not_a_file"
        );
    }

    #[test]
    fn bounds_previews_and_handles_binary_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data");
        std::fs::write(&path, [0, 255, 0, 255]).unwrap();
        assert_eq!(
            read_preview(None, path.to_str().unwrap()).unwrap().reason,
            Some("binary")
        );
        File::create(&path)
            .unwrap()
            .set_len(9 * 1024 * 1024)
            .unwrap();
        assert_eq!(
            read_preview(None, path.to_str().unwrap()).unwrap().reason,
            Some("tooLarge")
        );
    }
}
