//! People's pictures from the forge (`avatarUrl`) and the composer's link
//! favicons, fetched once per address with the system's `curl` (the app has
//! no HTTP client of its own) and kept for the session. Until one arrives,
//! or when it fails, the views draw the initials (or a globe), as the
//! desktop does on an image error.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, Context, Global, Image, ImageFormat};

#[derive(Clone)]
enum Avatar {
    Loading,
    Ready(Arc<Image>),
    Failed,
}

#[derive(Default)]
struct Avatars(HashMap<String, Avatar>);

impl Global for Avatars {}

/// Bigger answers are not a picture.
const MAX_BYTES: usize = 2 * 1024 * 1024;

fn format_of(bytes: &[u8]) -> Option<ImageFormat> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Png => Some(ImageFormat::Png),
        image::ImageFormat::Jpeg => Some(ImageFormat::Jpeg),
        image::ImageFormat::Gif => Some(ImageFormat::Gif),
        image::ImageFormat::WebP => Some(ImageFormat::Webp),
        _ => None,
    }
}

fn fetch(url: &str) -> Option<Arc<Image>> {
    if !url.starts_with("https://") {
        return None;
    }
    let output = std::process::Command::new("curl")
        .args(["-sSfL", "--max-time", "8", "--max-filesize", "2097152", url])
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() || output.stdout.len() > MAX_BYTES {
        return None;
    }
    let format = format_of(&output.stdout)?;
    Some(Arc::new(Image::from_bytes(format, output.stdout)))
}

/// The picture at `url` if it is here; otherwise it starts coming and the
/// view is drawn again when it does.
pub fn avatar<V: 'static>(url: Option<&str>, cx: &mut Context<V>) -> Option<Arc<Image>> {
    let url = url?.trim();
    if url.is_empty() {
        return None;
    }
    let known = cx.default_global::<Avatars>().0.get(url).cloned();
    match known {
        Some(Avatar::Ready(image)) => return Some(image),
        Some(Avatar::Loading | Avatar::Failed) => return None,
        None => {}
    }
    let url = url.to_string();
    cx.global_mut::<Avatars>()
        .0
        .insert(url.clone(), Avatar::Loading);
    cx.spawn(async move |this, cx| {
        let address = url.clone();
        let image = cx
            .background_executor()
            .spawn(async move { fetch(&address) })
            .await;
        cx.update(|cx: &mut App| {
            let state = image.map_or(Avatar::Failed, Avatar::Ready);
            cx.default_global::<Avatars>().0.insert(url, state);
        });
        let _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
    None
}

/// `forgeIdentityInitials`: up to two initials of a name or login.
pub fn initials(value: &str) -> String {
    let initials: String = value
        .split(|c: char| c.is_whitespace() || "@._/-".contains(c))
        .filter(|part| !part.trim().is_empty())
        .take(2)
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    if initials.is_empty() {
        "FG".to_string()
    } else {
        initials
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_the_first_two_words() {
        assert_eq!(initials("Wharley Ornelas"), "WO");
        assert_eq!(initials("@ana.lima"), "AL");
        assert_eq!(initials(""), "FG");
    }

    #[test]
    fn pictures_are_told_by_their_bytes() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert_eq!(format_of(&png), Some(ImageFormat::Png));
        assert_eq!(format_of(b"<html>"), None);
        assert!(fetch("http://example.com/a.png").is_none(), "https only");
    }
}
