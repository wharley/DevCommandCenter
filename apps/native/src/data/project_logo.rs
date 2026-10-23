//! `projectLogoFromFile`: an uploaded image becomes the small PNG data URI a
//! project logo is stored as, center-cropped to a square and scaled to 64px.

use std::path::Path;

use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbaImage, imageops::FilterType};

/// Side of the square PNG a project logo is stored as.
pub const LOGO_SIZE: u32 = 64;
/// Larger sources are refused before decoding.
const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024;
/// `PROJECT_LOGO_ACCEPT`.
pub const LOGO_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "svg"];

pub fn logo_from_file(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !LOGO_EXTENSIONS.contains(&extension.as_str()) {
        return Err("unsupported image type".into());
    }
    let size = std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err("image is too large".into());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let image = if extension == "svg" {
        rasterize_svg(&bytes)?
    } else {
        decode(&bytes)?
    };
    encode_logo(&image)
}

fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(bytes).map_err(|_| "image could not be decoded".to_string())
}

/// Draws an SVG at its own size, like an `<img>` the canvas then reads.
fn rasterize_svg(bytes: &[u8]) -> Result<DynamicImage, String> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .map_err(|_| "image could not be decoded".to_string())?;
    let size = tree.size().to_int_size();
    let (width, height) = (size.width().max(1), size.height().max(1));
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("image could not be decoded")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    // tiny-skia keeps premultiplied alpha; PNG wants it straight.
    let mut pixels = pixmap.take();
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel =
                    ((*channel as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8;
            }
        }
    }
    RgbaImage::from_raw(width, height, pixels)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| "image could not be decoded".to_string())
}

/// `squareCrop` then a 64px PNG as a data URI.
pub fn encode_logo(image: &DynamicImage) -> Result<String, String> {
    let (width, height) = (image.width().max(1), image.height().max(1));
    let side = width.min(height);
    let square = image.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    let logo = square.resize_exact(LOGO_SIZE, LOGO_SIZE, FilterType::CatmullRom);
    let mut png = Vec::new();
    logo.to_rgba8()
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_image_becomes_a_centered_64px_square() {
        let mut wide = RgbaImage::new(300, 100);
        for (x, _, pixel) in wide.enumerate_pixels_mut() {
            *pixel = if (100..200).contains(&x) {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            };
        }
        let uri = encode_logo(&DynamicImage::ImageRgba8(wide)).unwrap();
        let payload = uri.strip_prefix("data:image/png;base64,").unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        let logo = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(logo.dimensions(), (64, 64));
        // Only the red middle third is kept.
        assert_eq!(logo.get_pixel(32, 32).0, [255, 0, 0, 255]);
        assert!(uri.len() < 96 * 1024);
    }

    #[test]
    fn svg_logos_are_drawn() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#00ff00"/></svg>"##;
        let image = rasterize_svg(svg).unwrap();
        let uri = encode_logo(&image).unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
    }
}
