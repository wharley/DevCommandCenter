//! The two drawings: the providers' ring (`conic-gradient` from the top,
//! clockwise, 160px with the total in the middle) and the days' lines
//! (an SVG of 760×180 stretched to the card, dashed guides at a quarter,
//! half and three quarters, one 2px line per provider).

use dcc_core::domain::usage::{DailyUsageSummary, ProviderUsageSummary};
use gpui::{
    Bounds, Hsla, Path, PathBuilder, Pixels, Point, Window, canvas, div, point, prelude::*, px,
};

/// The value the drawings count: tokens when any were measured, else turns.
pub(super) fn metric(provider: &ProviderUsageSummary, tokens: bool) -> u64 {
    if tokens {
        provider.total_tokens
    } else {
        provider.turns
    }
}

/// Each provider's slice of the ring, in percent: (color, start, end).
pub(super) fn ring_stops(providers: &[ProviderUsageSummary], tokens: bool) -> Vec<(u32, f64, f64)> {
    let total: u64 = providers
        .iter()
        .map(|provider| metric(provider, tokens))
        .sum();
    let mut cursor = 0.0;
    providers
        .iter()
        .filter(|provider| metric(provider, tokens) > 0)
        .map(|provider| {
            let start = cursor;
            cursor += crate::data::usage::percent(metric(provider, tokens), total);
            (
                crate::data::usage::provider_color(&provider.provider_id),
                start,
                cursor,
            )
        })
        .collect()
}

/// The point at `percent` of the way round, from the top, clockwise.
fn on_circle(center: Point<Pixels>, radius: f32, percent: f64) -> Point<Pixels> {
    let angle = (percent / 100.0) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    point(
        center.x + px(radius * angle.cos() as f32),
        center.y + px(radius * angle.sin() as f32),
    )
}

fn paint_slice(bounds: Bounds<Pixels>, start: f64, end: f64, color: Hsla, window: &mut Window) {
    let center = bounds.center();
    let radius = f32::from(bounds.size.width) / 2.0;
    // A fan from the center: a point every 1% keeps the edge round.
    let steps = ((end - start).ceil() as usize).max(1) * 2;
    let mut slice = Path::new(center);
    for step in 0..=steps {
        let at = start + (end - start) * step as f64 / steps as f64;
        slice.line_to(on_circle(center, radius, at));
    }
    slice.line_to(center);
    window.paint_path(slice, color);
}

/// The ring (`size-40`), its slices or `bg-muted` when nothing counts.
pub(super) fn ring(stops: Vec<(u32, f64, f64)>, empty: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if stops.is_empty() {
                paint_slice(bounds, 0.0, 100.0, empty, window);
                return;
            }
            for (color, start, end) in &stops {
                paint_slice(
                    bounds,
                    *start,
                    *end,
                    crate::theme::color::hex(*color),
                    window,
                );
            }
        },
    )
    .size_full()
}

/// A provider's color and its points in the 760×180 box.
pub(super) type Line = (u32, Vec<(f32, f32)>);

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 180.0;
const PAD: f32 = 14.0;

/// One polyline per provider over the sorted days, in the 760×180 box.
pub(super) fn timeline_lines(
    daily: &[DailyUsageSummary],
    providers: &[ProviderUsageSummary],
    tokens: bool,
) -> (Vec<String>, Vec<Line>) {
    let mut dates: Vec<String> = daily.iter().map(|day| day.date.clone()).collect();
    dates.sort();
    dates.dedup();
    let value = |day: &DailyUsageSummary| {
        if tokens { day.total_tokens } else { day.turns }
    };
    let maximum = daily.iter().map(value).max().unwrap_or(0).max(1) as f32;
    let lines = providers
        .iter()
        .map(|provider| {
            let points = dates
                .iter()
                .enumerate()
                .map(|(index, date)| {
                    let x = if dates.len() <= 1 {
                        WIDTH / 2.0
                    } else {
                        PAD + (index as f32 / (dates.len() - 1) as f32) * (WIDTH - PAD * 2.0)
                    };
                    let amount = daily
                        .iter()
                        .find(|day| day.provider_id == provider.provider_id && &day.date == date)
                        .map(value)
                        .unwrap_or(0) as f32;
                    let y = HEIGHT - PAD - (amount / maximum) * (HEIGHT - PAD * 2.0);
                    (x, y)
                })
                .collect();
            (
                crate::data::usage::provider_color(&provider.provider_id),
                points,
            )
        })
        .collect();
    (dates, lines)
}

/// The lines stretched to the card (`preserveAspectRatio="none"`, the
/// strokes keep their width).
pub(super) fn timeline(lines: Vec<Line>, guide: Hsla) -> impl IntoElement {
    div().h(px(176.0)).w_full().child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let (scale_x, scale_y) = (
                    f32::from(bounds.size.width) / WIDTH,
                    f32::from(bounds.size.height) / HEIGHT,
                );
                let at = |x: f32, y: f32| {
                    point(
                        bounds.origin.x + px(x * scale_x),
                        bounds.origin.y + px(y * scale_y),
                    )
                };
                for ratio in [0.25, 0.5, 0.75] {
                    let mut line = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(5.0)]);
                    line.move_to(at(PAD, HEIGHT * ratio));
                    line.line_to(at(WIDTH - PAD, HEIGHT * ratio));
                    if let Ok(path) = line.build() {
                        window.paint_path(path, guide);
                    }
                }
                for (color, points) in &lines {
                    let mut stroke = PathBuilder::stroke(px(2.0));
                    for (index, (x, y)) in points.iter().enumerate() {
                        if index == 0 {
                            stroke.move_to(at(*x, *y));
                        } else {
                            stroke.line_to(at(*x, *y));
                        }
                    }
                    if points.len() > 1
                        && let Ok(path) = stroke.build()
                    {
                        window.paint_path(path, crate::theme::color::hex(*color));
                    }
                }
            },
        )
        .size_full(),
    )
}
