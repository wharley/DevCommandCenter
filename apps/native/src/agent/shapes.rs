//! The mascot's silhouettes and accessories (`AgentAvatar::shape` and
//! `accessory`), in the same 100-unit box as the octopus: each shape says
//! where its eyes sit and where an accessory goes on it. The phone draws the
//! same elements (`packages/mobile-core/src/residentAgents.ts`).

const INK: &str = "#0b1220";
const CAP: &str = "#1f2937";
const ANTENNA_TIP: &str = "#F2545B";

/// How a shape is drawn and where things go on it.
pub(super) struct Silhouette {
    /// The body, in `fill`.
    pub body: String,
    /// How far down its eyes sit from the octopus' (46).
    pub eyes_dy: f32,
    /// Only the octopus raises an arm when it needs the person.
    pub raises_arm: bool,
}

/// Top of the head and its half width where a cap sits; where an antenna
/// starts.
struct Head {
    cap_top: f32,
    cap_half: f32,
    antenna_base: f32,
}

fn head(shape: &str) -> Head {
    match shape {
        "drop" => Head {
            cap_top: 22.0,
            cap_half: 15.0,
            antenna_base: 14.0,
        },
        "block" => Head {
            cap_top: 16.0,
            cap_half: 27.0,
            antenna_base: 19.0,
        },
        _ => Head {
            cap_top: 12.0,
            cap_half: 24.0,
            antenna_base: 15.0,
        },
    }
}

/// The body of `shape` (`octopus` counts `arms`).
pub(super) fn silhouette(shape: &str, arms: u8, fill: &str) -> Silhouette {
    let shine = |x: f32, y: f32| {
        format!(
            r##"<ellipse cx="{x}" cy="{y}" rx="9" ry="5" fill="#fff" opacity="0.22" transform="rotate(-28 {x} {y})"/>"##
        )
    };
    match shape {
        "drop" => Silhouette {
            body: format!(
                r##"<path d="M50 8C64 30 82 46 82 64a32 32 0 0 1-64 0C18 46 36 30 50 8z" fill="{fill}"/>{}"##,
                shine(40.0, 46.0)
            ),
            eyes_dy: 16.0,
            raises_arm: false,
        },
        "block" => Silhouette {
            body: format!(
                r##"<rect x="17" y="18" width="66" height="66" rx="18" fill="{fill}"/>{}"##,
                shine(32.0, 30.0)
            ),
            eyes_dy: 6.0,
            raises_arm: false,
        },
        "ghost" => Silhouette {
            body: format!(
                r##"<path d="M16 86V50C16 29 32 14 50 14s34 15 34 36v36l-8.5-7-8.5 7-8.5-7-8.5 7-8.5-7-8.5 7-8.5-7z" fill="{fill}"/>{}"##,
                shine(36.0, 27.0)
            ),
            eyes_dy: 0.0,
            raises_arm: false,
        },
        _ => {
            let arms = arms.clamp(3, 5) as usize;
            let gap = 3.0;
            let width = (68.0 - gap * (arms as f32 - 1.0)) / arms as f32;
            let mut body = String::new();
            for index in 0..arms {
                let height = if index % 2 == 0 { 26 } else { 20 };
                body.push_str(&format!(
                    r##"<rect x="{}" y="56" width="{width}" height="{height}" rx="{}" fill="{fill}"/>"##,
                    16.0 + index as f32 * (width + gap),
                    width / 2.0
                ));
            }
            body.push_str(&format!(
                r##"<path d="M16 60V52C16 29 32 14 50 14s34 15 34 38v8z" fill="{fill}"/>"##
            ));
            body.push_str(&shine(36.0, 27.0));
            Silhouette {
                body,
                eyes_dy: 0.0,
                raises_arm: true,
            }
        }
    }
}

/// What the mascot wears or holds, drawn over its face.
pub(super) fn accessory(accessory: &str, shape: &str, eyes_dy: f32) -> String {
    let head = head(shape);
    let eyes = 46.0 + eyes_dy;
    match accessory {
        "glasses" => format!(
            r##"<g fill="none" stroke="{INK}" stroke-width="3.5"><circle cx="39" cy="{eyes}" r="10.5"/><circle cx="61" cy="{eyes}" r="10.5"/><path d="M49.5 {eyes}h1" stroke-linecap="round"/></g>"##
        ),
        "magnifier" => format!(
            r##"<circle cx="20" cy="24" r="9" fill="#fff" fill-opacity="0.45" stroke="{INK}" stroke-width="4"/><path d="M13.5 30.5l-7 7" stroke="{INK}" stroke-width="5" stroke-linecap="round"/>"##
        ),
        "antenna" => {
            let base = head.antenna_base;
            format!(
                r##"<path d="M50 {base}v-9" stroke="{INK}" stroke-width="3.5" stroke-linecap="round"/><circle cx="50" cy="{}" r="4.5" fill="{ANTENNA_TIP}"/>"##,
                base - 11.0
            )
        }
        "cap" => {
            let (top, half) = (head.cap_top, head.cap_half);
            format!(
                r##"<path d="M{} {}Q50 {} {} {}z" fill="{CAP}"/><path d="M{} {}h14" stroke="{CAP}" stroke-width="4" stroke-linecap="round"/>"##,
                50.0 - half,
                top + 15.0,
                top - 12.0,
                50.0 + half,
                top + 15.0,
                50.0 + half - 6.0,
                top + 15.0
            )
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_and_accessory_draws() {
        for shape in dcc_core::domain::agent::AGENT_AVATAR_SHAPES {
            let silhouette = silhouette(shape, 4, "#356EFF");
            assert!(!silhouette.body.is_empty(), "{shape}");
            for item in dcc_core::domain::agent::AGENT_AVATAR_ACCESSORIES {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{}{}</svg>"##,
                    silhouette.body,
                    accessory(item, shape, silhouette.eyes_dy)
                );
                assert!(
                    resvg::usvg::Tree::from_str(&svg, &Default::default()).is_ok(),
                    "{shape} {item}"
                );
            }
        }
        assert!(accessory("none", "octopus", 0.0).is_empty());
        assert!(silhouette("octopus", 4, "#000").raises_arm);
        assert!(!silhouette("drop", 4, "#000").raises_arm);
    }
}
