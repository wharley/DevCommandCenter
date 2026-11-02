//! The Device panel headlessly: the list while none is open, a decoded
//! simulator frame drawn at the screen's proportions, the pointer as 0..1 of
//! the screen; and, by hand, a real simulator end to end.

use gpui::{AppContext, Bounds, TestAppContext, WindowAppearance, point, px, size};

use super::{DevicePanel, Phase};
use crate::data::device::Simulator;
use crate::data::device::decode::Decoder;
use crate::data::device::stream::{Chunk, Reader};
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<DevicePanel>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(|_, cx| DevicePanel::new(cx))
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// The first picture of the simulator's stream in the fixture.
fn first_picture() -> crate::data::device::decode::Picture {
    let stream = include_bytes!("../../data/device/testdata/iphone-16-pro.avcc");
    let chunks = Reader::default().push(stream).unwrap();
    let Chunk::Description(avcc) = &chunks[0] else {
        panic!("description");
    };
    let Chunk::Frame { data, .. } = &chunks[1] else {
        panic!("keyframe");
    };
    Decoder::new(avcc).unwrap().decode(data).unwrap()
}

#[gpui::test]
fn the_list_shows_until_a_simulator_is_live_at_its_proportions(cx: &mut TestAppContext) {
    let (panel, cx) = open(cx);
    panel.update(cx, |panel, cx| {
        panel.simulators = vec![Simulator {
            udid: "47B9DDB6-8CF6-4866-9A60-B4D1F6EC0B4D".into(),
            name: "iPhone 16 Pro".into(),
            runtime: "iOS 18.3".into(),
            booted: false,
            ipad: false,
            last_booted: None,
        }];
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("device-panel").is_some());
    let bar = cx.debug_bounds("device-bar").expect("the bar");
    assert_eq!(
        bar.size.height,
        px(crate::screens::conversation::header::HEIGHT),
        "its line meets the conversation header's"
    );
    assert!(cx.debug_bounds("device-screen").is_none(), "the list first");

    panel.update(cx, |panel, cx| {
        panel.current = panel.simulators.first().cloned();
        panel.phase = Phase::Live;
        panel.size = (1206, 2622);
        panel.picture = Some(first_picture());
        cx.notify();
    });
    draw(cx);
    let screen = cx.debug_bounds("device-screen").expect("the screen");
    let ratio = f32::from(screen.size.height) / f32::from(screen.size.width);
    assert!((ratio - 2622.0 / 1206.0).abs() < 0.02, "{screen:?}");
    assert!(screen.size.width <= px(super::view::WIDTH));
    // The pointer maps over the screen itself, not below it.
    let touch = panel.read_with(cx, |panel, _| panel.screen.get());
    assert_eq!(touch, screen, "the touch area is the screen");
}

#[gpui::test]
fn the_pointer_is_a_share_of_the_screen(cx: &mut TestAppContext) {
    let (panel, cx) = open(cx);
    panel.update(cx, |panel, _| {
        panel.screen.set(Bounds::new(
            point(px(100.0), px(50.0)),
            size(px(200.0), px(400.0)),
        ));
        assert_eq!(panel.unit(point(px(150.0), px(150.0))), (0.25, 0.25));
        assert_eq!(
            panel.unit(point(px(400.0), px(0.0))),
            (1.0, 0.0),
            "clamped to the screen"
        );
    });
}

/// A booted-or-not iPhone driven through the app's own helper. Needs Xcode
/// and the built bundle: `DCC_CLAUDE_SIDECAR_PATH=<bundle>/Contents/MacOS/
/// dcc-claude-sidecar DCC_DEVICE_UDID=<udid> cargo test real_simulator -- --ignored`.
#[test]
#[ignore = "boots a real simulator"]
fn real_simulator_streams_through_the_helper() {
    use futures::StreamExt;

    use crate::data::device::link::{DeviceLink, LinkEvent};

    let udid = std::env::var("DCC_DEVICE_UDID").expect("DCC_DEVICE_UDID");
    let helper = crate::data::device::helper().expect("DCC_CLAUDE_SIDECAR_PATH");
    crate::data::device::boot(&udid).unwrap();
    let (link, mut events) = DeviceLink::open(&helper, &udid).unwrap();
    let mut decoder = None;
    let mut pictures = 0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while pictures < 2 && std::time::Instant::now() < deadline {
        match futures::executor::block_on(events.next()) {
            Some(LinkEvent::Description(avcc)) => decoder = Decoder::new(&avcc).ok(),
            Some(LinkEvent::Frame { data, .. }) => {
                if decoder.as_mut().and_then(|d| d.decode(&data)).is_some() {
                    pictures += 1;
                }
                if pictures == 1 {
                    link.send(serde_json::json!({"op": "button", "name": "home"}));
                }
            }
            Some(_) => {}
            None => break,
        }
    }
    assert!(pictures >= 2, "decoded {pictures} pictures");
    let tree = futures::executor::block_on(link.request(serde_json::json!({"op": "describe"})));
    assert!(tree.is_ok(), "{tree:?}");
}

/// Only a simulator the panel booted goes down when the panel leaves it.
#[gpui::test]
fn the_panel_shuts_down_only_what_it_booted(cx: &mut TestAppContext) {
    let (panel, cx) = open(cx);
    let sim = |udid: &str| Simulator {
        udid: udid.into(),
        name: "iPhone 16 Pro".into(),
        runtime: "iOS 18.3".into(),
        booted: true,
        ipad: false,
        last_booted: None,
    };
    panel.update(cx, |panel, _| {
        panel.current = Some(sim("XCODE"));
        assert_eq!(panel.leave(), None, "Xcode's stays running");
        panel.current = Some(sim("OURS"));
        panel.started.insert("OURS".into());
        assert_eq!(panel.leave().as_deref(), Some("OURS"));
        assert_eq!(panel.leave(), None, "once");
    });
}

/// The panel drawn by Metal, saved to look at:
/// `DCC_DEVICE_PICTURE=/tmp/x.png cargo test device_panel_picture -- --ignored`.
#[test]
#[ignore = "renders with the GPU"]
fn device_panel_picture() {
    use std::sync::Arc;

    let out = std::env::var("DCC_DEVICE_PICTURE").expect("DCC_DEVICE_PICTURE");
    let mut cx = gpui::HeadlessAppContext::with_platform(
        Arc::new(gpui_apple::AppleTextSystem::new()),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let window = cx
        .open_window(size(px(400.0), px(900.0)), |_, cx| {
            cx.new(|cx| {
                let mut panel = DevicePanel::new(cx);
                panel.current = Some(Simulator {
                    udid: "47B9DDB6-8CF6-4866-9A60-B4D1F6EC0B4D".into(),
                    name: "iPhone 16 Pro".into(),
                    runtime: "iOS 18.1".into(),
                    booted: true,
                    ipad: false,
                    last_booted: None,
                });
                panel.phase = Phase::Live;
                panel.size = (1206, 2622);
                panel.picture = Some(first_picture());
                panel
            })
        })
        .unwrap();
    // Twice: the second layout uses the room the first measured.
    for _ in 0..2 {
        cx.update_window(window.into(), |_, window, _| window.refresh())
            .unwrap();
        cx.run_until_parked();
    }
    let image = cx.capture_screenshot(window.into()).unwrap();
    image.save(&out).unwrap();
}
