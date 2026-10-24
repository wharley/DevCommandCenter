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

/// CSS `color-mix(in srgb, a weight_a%, b)`: straight over the sRGB
/// channels (premultiplied by alpha, as CSS mixes).
pub fn mix_srgb(a: Hsla, weight_a: f32, b: Hsla) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let t = weight_a.clamp(0.0, 1.0);
    let alpha = a.a * t + b.a * (1.0 - t);
    if alpha <= 0.0 {
        return gpui::transparent_black();
    }
    let channel = |x: f32, y: f32| (x * a.a * t + y * b.a * (1.0 - t)) / alpha;
    Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: alpha,
    }
    .into()
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

/// CSS `color-mix(in lab, a weight_a%, b)`: CIE Lab under D50, as CSS
/// Color 4 defines `lab` (the desktop's diff colors mix this way).
pub fn mix_lab(a: Hsla, weight_a: f32, b: Hsla) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let t = weight_a.clamp(0.0, 1.0);
    let la = rgba_to_lab(a);
    let lb = rgba_to_lab(b);
    let mixed = [
        la[0] * t + lb[0] * (1.0 - t),
        la[1] * t + lb[1] * (1.0 - t),
        la[2] * t + lb[2] * (1.0 - t),
    ];
    let mut rgba = lab_to_rgba(mixed);
    rgba.a = a.a * t + b.a * (1.0 - t);
    rgba.into()
}

/// D50 white (CSS Color 4).
const D50: [f32; 3] = [0.964_22, 1.0, 0.825_21];

fn rgba_to_lab(color: Rgba) -> [f32; 3] {
    let (r, g, b) = (
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
    );
    // linear sRGB → XYZ (D65) → XYZ (D50, Bradford), the CSS Color 4 matrices.
    let x65 = 0.412_390_8 * r + 0.357_584_33 * g + 0.180_480_8 * b;
    let y65 = 0.212_639 * r + 0.715_168_7 * g + 0.072_192_32 * b;
    let z65 = 0.019_330_82 * r + 0.119_194_78 * g + 0.950_532_14 * b;
    let x = 1.047_929_8 * x65 + 0.022_946_793 * y65 - 0.050_192_23 * z65;
    let y = 0.029_627_815 * x65 + 0.990_434_5 * y65 - 0.017_073_825 * z65;
    let z = -0.009_243_058 * x65 + 0.015_055_144 * y65 + 0.751_874_3 * z65;
    let f = |value: f32| {
        const E: f32 = 216.0 / 24389.0;
        const K: f32 = 24389.0 / 27.0;
        if value > E {
            value.cbrt()
        } else {
            (K * value + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x / D50[0]), f(y / D50[1]), f(z / D50[2]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn lab_to_rgba(lab: [f32; 3]) -> Rgba {
    const E: f32 = 216.0 / 24389.0;
    const K: f32 = 24389.0 / 27.0;
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = lab[1] / 500.0 + fy;
    let fz = fy - lab[2] / 200.0;
    let x = if fx.powi(3) > E {
        fx.powi(3)
    } else {
        (116.0 * fx - 16.0) / K
    };
    let y = if lab[0] > K * E {
        fy.powi(3)
    } else {
        lab[0] / K
    };
    let z = if fz.powi(3) > E {
        fz.powi(3)
    } else {
        (116.0 * fz - 16.0) / K
    };
    let (x, y, z) = (x * D50[0], y * D50[1], z * D50[2]);
    // XYZ (D50) → XYZ (D65) → linear sRGB.
    let x65 = 0.955_473_4 * x - 0.023_098_538 * y + 0.063_259_31 * z;
    let y65 = -0.028_369_706 * x + 1.009_995_4 * y + 0.021_041_399 * z;
    let z65 = 0.012_314_002 * x - 0.020_507_696 * y + 1.330_365_9 * z;
    let r = 3.240_97 * x65 - 1.537_383_2 * y65 - 0.498_610_76 * z65;
    let g = -0.969_243_6 * x65 + 1.875_967_5 * y65 + 0.041_555_06 * z65;
    let b = 0.055_630_08 * x65 - 0.203_976_96 * y65 + 1.056_971_5 * z65;
    Rgba {
        r: linear_to_srgb(r),
        g: linear_to_srgb(g),
        b: linear_to_srgb(b),
        a: 1.0,
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
mod lab_tests {
    use super::*;

    #[test]
    fn lab_mixing_matches_css() {
        // What Chromium answers for `color-mix(in lab, …)` with the same colors.
        let channel = |value: f32| (value * 255.0).round() as i32;
        for (a, weight, b, expected) in [
            (0xffffff, 0.88, 0x0dbe4e, [233, 248, 233]),
            (0x0a0a0a, 0.80, 0x5ecc71, [30, 46, 31]),
            (0x0a0a0a, 0.65, 0xfafafa, [85, 85, 85]),
            (0xffffff, 0.91, 0xff2e3f, [255, 239, 236]),
        ] {
            let mixed = mix_lab(hex(a), weight, hex(b)).to_rgb();
            let got = [channel(mixed.r), channel(mixed.g), channel(mixed.b)];
            assert!(
                got.iter()
                    .zip(expected)
                    .all(|(got, expected)| (got - expected).abs() <= 1),
                "{a:06x} {weight} {b:06x}: {got:?} vs {expected:?}"
            );
        }
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
