//! Links in the composer's text (`matchComposerUrl` and
//! `ComposerLinkIconsPlugin`): an `http(s)://` or `www.` address is drawn
//! blue on a faint fill with the site's favicon before it (a globe until it
//! comes). The icon is only drawn: the text, the draft and what is sent stay
//! the same. ⌘-click opens the link.

use std::ops::Range;
use std::sync::{Arc, LazyLock};

use gpui::{AnyElement, App, Hsla, Image, Pixels, Point, Window, img, point, prelude::*, px};
use regex::Regex;

use crate::theme::color::hex;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::Icon;

/// The icon's side and the gap after it (`1em`, `0.3em`).
pub const ICON: f32 = 14.0;
pub const GAP: f32 = 4.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub range: Range<usize>,
    pub url: String,
}

/// `.composer-link`: #0969da, #79b8ff in dark.
pub fn color(cx: &App) -> Hsla {
    match cx.theme().appearance {
        Appearance::Dark => hex(0x79B8FF),
        Appearance::Light => hex(0x0969DA),
    }
}

/// `matchComposerUrl`: every address in `text`, its trailing punctuation
/// and unbalanced `)` left to the prose around it.
pub fn find(text: &str) -> Vec<Link> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:https?://|www\.)[^\s<>\x{FFFC}]+").expect("link pattern")
    });
    PATTERN
        .find_iter(text)
        .filter_map(|found| {
            let mut value = found
                .as_str()
                .trim_end_matches(['.', ',', '!', '?', ';', ':']);
            while value.ends_with(')') && value.matches(')').count() > value.matches('(').count() {
                value = &value[..value.len() - 1];
            }
            let url = normalize(value)?;
            Some(Link {
                range: found.start()..found.start() + value.len(),
                url,
            })
        })
        .collect()
}

/// `normalizeComposerUrl`: `www.` gets `https://`; only http(s) with a host.
fn normalize(value: &str) -> Option<String> {
    let full = if value
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case("www."))
    {
        format!("https://{value}")
    } else {
        value.to_string()
    };
    let url = url::Url::parse(&full).ok()?;
    let host = url.host_str().filter(|host| !host.is_empty());
    (matches!(url.scheme(), "http" | "https") && host.is_some()).then(|| url.to_string())
}

/// `siteFaviconSource`: only a public host name goes to the icon service,
/// never the path or a token in it.
pub fn favicon_source(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.trim_end_matches('.');
    let private = [".localhost", ".local", ".internal", ".test", ".invalid"];
    if !host.contains('.')
        || host.chars().all(|c| c.is_ascii_digit() || c == '.')
        || host.contains(':')
        || private.iter().any(|suffix| host.ends_with(suffix))
    {
        return None;
    }
    Some(format!(
        "https://www.google.com/s2/favicons?domain={host}&sz=32"
    ))
}

/// The favicon, or the globe while it loads or when the site has none.
pub fn icon(favicon: Option<Arc<Image>>, cx: &App) -> AnyElement {
    match favicon {
        Some(image) => img(image)
            .size(px(ICON))
            .rounded(px(2.0))
            .into_any_element(),
        None => Icon::new("globe")
            .size(ICON)
            .color(color(cx))
            .into_any_element(),
    }
}

/// Lays the icon out at the link's start (`at`, in window coordinates).
pub fn prepaint_icon(
    icon: &mut AnyElement,
    at: Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let origin = point(at.x + px(2.0), at.y + (line_height - px(ICON)) / 2.0);
    icon.prepaint_as_root(origin, gpui::size(px(ICON), px(ICON)).into(), window, cx);
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, WindowAppearance};

    use super::*;
    use crate::theme::{Theme, ThemePreference};
    use crate::ui::text_area::TextArea;

    /// A local address: marked, but no icon is fetched.
    #[gpui::test]
    fn a_pasted_link_is_marked_and_the_text_stays(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        });
        let (area, cx) = cx.add_window_view(|window, cx| TextArea::new(window, cx).with_lists());
        let text = "veja http://localhost:3000/x ok";
        area.update(cx, |area, cx| area.set_text(text, cx));
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        area.read_with(cx, |area, _| {
            assert_eq!(area.text(), text);
            assert_eq!(area.links.len(), 1);
            assert_eq!(area.links[0].0.range, 5..28);
            let layout = area.layout.as_ref().unwrap();
            let before = layout.position_for(5);
            let first = layout.position_for(6);
            assert!(
                first.x - before.x > px(ICON),
                "the icon holds room before the link"
            );
        });
    }

    fn urls(text: &str) -> Vec<(&str, String)> {
        find(text)
            .into_iter()
            .map(|link| (&text[link.range], link.url))
            .collect()
    }

    #[test]
    fn addresses_leave_the_prose_around_them() {
        assert_eq!(
            urls("veja https://github.com/a/b."),
            vec![("https://github.com/a/b", "https://github.com/a/b".into())]
        );
        assert_eq!(
            urls("(em www.rust-lang.org) e http://x.dev/a_(b)!"),
            vec![
                ("www.rust-lang.org", "https://www.rust-lang.org/".into()),
                ("http://x.dev/a_(b)", "http://x.dev/a_(b)".into()),
            ]
        );
        assert!(urls("ftp://x.com e https://").is_empty());
        let chip = format!("https://a.com{}", crate::ui::text_area::CHIP);
        assert_eq!(urls(&chip)[0].0, "https://a.com");
    }

    #[test]
    fn only_public_hosts_ask_for_an_icon() {
        assert_eq!(
            favicon_source("https://github.com/org/repo?token=x").as_deref(),
            Some("https://www.google.com/s2/favicons?domain=github.com&sz=32")
        );
        for private in [
            "http://localhost:3000",
            "http://127.0.0.1/a",
            "https://app.local/",
            "https://api.test/x",
            "http://[::1]/",
        ] {
            assert_eq!(favicon_source(private), None, "{private}");
        }
    }
}
