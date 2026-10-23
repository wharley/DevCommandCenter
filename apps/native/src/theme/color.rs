//! CSS color math the desktop tokens rely on: `oklch()` values and
//! `color-mix(in oklab, …)`, converted to GPUI colors.

use gpui::{Hsla, Rgba};

/// A color in the OKLab space, the space CSS `color-mix(in oklab, …)` interpolates in.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Oklab {
    l: f32,
    a: f32,
    b: f32,
    alpha: f32,
}

/// CSS `oklch(l c h)`; `h` in degrees.
pub fn oklch(l: f32, c: f32, h: f32) -> Hsla {
    oklch_alpha(l, c, h, 1.0)
}

/// CSS `oklch(l c h / alpha)`.
pub fn oklch_alpha(l: f32, c: f32, h: f32, alpha: f32) -> Hsla {
    oklab_to_rgba(oklch_to_oklab(l, c, h, alpha)).into()
}

/// CSS hex color (`#rrggbb`) as a GPUI color.
pub fn hex(rgb: u32) -> Hsla {
    gpui::rgb(rgb).into()
}

/// CSS `rgb(r, g, b)` with 0–255 channels.
pub fn rgb_triplet(r: u8, g: u8, b: u8) -> Hsla {
    hex((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

/// CSS `color-mix(in oklab, a weight_a%, b)`: `weight_a` is the share of `a`, from 0 to 1.
pub fn mix_oklab(a: Hsla, weight_a: f32, b: Hsla) -> Hsla {
    let a = rgba_to_oklab(a.to_rgb());
    let b = rgba_to_oklab(b.to_rgb());
    let t = weight_a.clamp(0.0, 1.0);
    let mixed = Oklab {
        l: a.l * t + b.l * (1.0 - t),
        a: a.a * t + b.a * (1.0 - t),
        b: a.b * t + b.b * (1.0 - t),
        alpha: a.alpha * t + b.alpha * (1.0 - t),
    };
    oklab_to_rgba(mixed).into()
}

fn oklch_to_oklab(l: f32, c: f32, h: f32, alpha: f32) -> Oklab {
    let radians = h.to_radians();
    Oklab {
        l,
        a: c * radians.cos(),
        b: c * radians.sin(),
        alpha,
    }
}

fn oklab_to_rgba(color: Oklab) -> Rgba {
    let l_ = color.l + 0.396_337_78 * color.a + 0.215_803_76 * color.b;
    let m_ = color.l - 0.105_561_346 * color.a - 0.063_854_17 * color.b;
    let s_ = color.l - 0.089_484_18 * color.a - 1.291_485_5 * color.b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
    let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;

    Rgba {
        r: linear_to_srgb(r),
        g: linear_to_srgb(g),
        b: linear_to_srgb(b),
        a: color.alpha.clamp(0.0, 1.0),
    }
}

fn rgba_to_oklab(color: Rgba) -> Oklab {
    let r = srgb_to_linear(color.r);
    let g = srgb_to_linear(color.g);
    let b = srgb_to_linear(color.b);

    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();

    Oklab {
        l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        alpha: color.a,
    }
}

fn linear_to_srgb(channel: f32) -> f32 {
    let channel = channel.clamp(0.0, 1.0);
    if channel <= 0.003_130_8 {
        12.92 * channel
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_hex(color: Hsla) -> u32 {
        let rgba = color.to_rgb();
        let byte = |v: f32| (v * 255.0).round() as u32;
        (byte(rgba.r) << 16) | (byte(rgba.g) << 8) | byte(rgba.b)
    }

    #[test]
    fn neutral_scale_matches_tailwind() {
        // Tailwind's neutral palette is defined by these exact oklch values.
        assert_eq!(to_hex(oklch(0.985, 0.0, 0.0)), 0xfafafa);
        assert_eq!(to_hex(oklch(0.97, 0.0, 0.0)), 0xf5f5f5);
        assert_eq!(to_hex(oklch(0.922, 0.0, 0.0)), 0xe5e5e5);
        assert_eq!(to_hex(oklch(0.708, 0.0, 0.0)), 0xa1a1a1);
        assert_eq!(to_hex(oklch(0.556, 0.0, 0.0)), 0x737373);
        assert_eq!(to_hex(oklch(0.269, 0.0, 0.0)), 0x262626);
        assert_eq!(to_hex(oklch(0.205, 0.0, 0.0)), 0x171717);
        assert_eq!(to_hex(oklch(0.145, 0.0, 0.0)), 0x0a0a0a);
    }

    #[test]
    fn chromatic_values_land_on_tailwind_hex() {
        // red-600, the light destructive token
        let red = to_hex(oklch(0.577, 0.245, 27.325));
        assert!(close_hex(red, 0xe7000b), "got {red:06x}");
    }

    #[test]
    fn alpha_is_kept() {
        let border = oklch_alpha(1.0, 0.0, 0.0, 0.1);
        assert!((border.a - 0.1).abs() < 1e-6);
        assert_eq!(to_hex(border), 0xffffff);
    }

    #[test]
    fn mix_in_oklab_hits_the_endpoints() {
        let a = oklch(0.985, 0.0, 0.0);
        let b = hex(0xac751c);
        assert_eq!(to_hex(mix_oklab(a, 1.0, b)), to_hex(a));
        assert_eq!(to_hex(mix_oklab(a, 0.0, b)), to_hex(b));
        let mid = to_hex(mix_oklab(a, 0.97, b));
        assert_ne!(mid, to_hex(a));
    }

    fn close_hex(a: u32, b: u32) -> bool {
        let channels = |v: u32| [(v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff];
        channels(a)
            .iter()
            .zip(channels(b))
            .all(|(x, y)| x.abs_diff(y) <= 1)
    }
}
