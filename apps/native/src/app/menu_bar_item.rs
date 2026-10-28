//! The DCC item in the macOS menu bar (`menu_bar_macos.m`): the four flows
//! of the app icon in one template stroke, so it takes the bar's own
//! contrast in light and dark; "Dev Command Center" as its tooltip; a click
//! hands the button's place on screen to the app, which opens the panel
//! under it.

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject};
use objc2::{MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

/// Where the button sits, in the app's coordinates (top left of the main screen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonPlace {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

type OnClick = Box<dyn Fn(ButtonPlace)>;

thread_local! {
    static ON_CLICK: RefCell<Option<OnClick>> = const { RefCell::new(None) };
    static ITEM: RefCell<Option<(Retained<AnyObject>, Retained<Target>)>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Target` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DCCNativeMenuBarTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(toggle:))]
        fn toggle(&self, sender: &AnyObject) {
            let place = unsafe { button_place(sender) };
            ON_CLICK.with(|click| {
                if let (Some(click), Some(place)) = (click.borrow().as_ref(), place) {
                    click(place);
                }
            });
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// The status item window's frame, flipped to top-left coordinates.
unsafe fn button_place(button: &AnyObject) -> Option<ButtonPlace> {
    unsafe {
        let window: Option<Retained<AnyObject>> = msg_send![button, window];
        let frame: NSRect = msg_send![&*window?, frame];
        let screens: Retained<AnyObject> = msg_send![class!(NSScreen), screens];
        let main: Option<Retained<AnyObject>> = msg_send![&*screens, firstObject];
        let screen: NSRect = msg_send![&*main?, frame];
        Some(ButtonPlace {
            x: frame.origin.x as f32,
            y: (screen.size.height - frame.origin.y - frame.size.height) as f32,
            width: frame.size.width as f32,
            height: frame.size.height as f32,
        })
    }
}

/// `dccMenuBarImage`: 18pt, the app icon's paths scaled from its 552 units.
fn menu_bar_image() -> Retained<AnyObject> {
    let draw = RcBlock::new(|_: NSRect| -> Bool {
        unsafe {
            let transform: Retained<AnyObject> = msg_send![class!(NSAffineTransform), transform];
            let _: () = msg_send![&*transform, translateXBy: 1.0f64, yBy: 17.0f64];
            let _: () = msg_send![&*transform, scaleXBy: 16.0f64 / 552.0, yBy: -16.0f64 / 552.0];
            let _: () = msg_send![&*transform, translateXBy: -236.0f64, yBy: -236.0f64];
            let _: () = msg_send![&*transform, concat];
            let black: Retained<AnyObject> = msg_send![class!(NSColor), blackColor];
            let _: () = msg_send![&*black, setStroke];
            let path: Retained<AnyObject> = msg_send![class!(NSBezierPath), bezierPath];
            let _: () = msg_send![&*path, setLineWidth: 60.0f64];
            // NSLineCapStyleRound
            let _: () = msg_send![&*path, setLineCapStyle: 1usize];
            let point = |x: f64, y: f64| NSPoint::new(x, y);
            let move_to = |x, y| {
                let _: () = msg_send![&*path, moveToPoint: point(x, y)];
            };
            let line_to = |x, y| {
                let _: () = msg_send![&*path, lineToPoint: point(x, y)];
            };
            let curve = |to: (f64, f64), one: (f64, f64), two: (f64, f64)| {
                let _: () = msg_send![
                    &*path,
                    curveToPoint: point(to.0, to.1),
                    controlPoint1: point(one.0, one.1),
                    controlPoint2: point(two.0, two.1)
                ];
            };
            move_to(300.0, 326.0);
            line_to(352.0, 326.0);
            curve((430.0, 432.0), (416.0, 326.0), (430.0, 384.0));
            curve((500.0, 542.0), (430.0, 480.0), (461.0, 505.0));
            curve((716.0, 710.0), (590.0, 640.0), (590.0, 710.0));
            move_to(724.0, 326.0);
            line_to(672.0, 326.0);
            curve((594.0, 432.0), (608.0, 326.0), (594.0, 384.0));
            curve((524.0, 542.0), (594.0, 480.0), (563.0, 505.0));
            curve((308.0, 710.0), (434.0, 640.0), (434.0, 710.0));
            move_to(512.0, 270.0);
            line_to(512.0, 754.0);
            move_to(270.0, 512.0);
            line_to(754.0, 512.0);
            let _: () = msg_send![&*path, stroke];
        }
        Bool::YES
    });
    unsafe {
        let image: Retained<AnyObject> = msg_send![
            class!(NSImage),
            imageWithSize: NSSize::new(18.0, 18.0),
            flipped: Bool::NO,
            drawingHandler: &*draw
        ];
        let _: () = msg_send![&*image, setTemplate: Bool::YES];
        image
    }
}

/// `dcc_menu_bar_create`: false off the main thread or when AppKit refuses.
pub fn install(on_click: impl Fn(ButtonPlace) + 'static) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    if ITEM.with(|item| item.borrow().is_some()) {
        return true;
    }
    unsafe {
        let bar: Retained<AnyObject> = msg_send![class!(NSStatusBar), systemStatusBar];
        // NSSquareStatusItemLength
        let item: Retained<AnyObject> = msg_send![&*bar, statusItemWithLength: -2.0f64];
        let button: Option<Retained<AnyObject>> = msg_send![&*item, button];
        let Some(button) = button else {
            return false;
        };
        let image = menu_bar_image();
        let _: () = msg_send![&*button, setImage: &*image];
        let tooltip = NSString::from_str("Dev Command Center");
        let _: () = msg_send![&*button, setToolTip: &*tooltip];
        let label = NSString::from_str("DCC");
        let _: () = msg_send![&*button, setAccessibilityLabel: &*label];
        let target = Target::new(mtm);
        let _: () = msg_send![&*button, setTarget: &*target];
        let _: () = msg_send![&*button, setAction: sel!(toggle:)];
        ITEM.with(|slot| *slot.borrow_mut() = Some((item, target)));
    }
    ON_CLICK.with(|click| *click.borrow_mut() = Some(Box::new(on_click)));
    true
}

/// Whether the item is in the bar (closing the window then hides it).
pub fn installed() -> bool {
    ITEM.with(|item| item.borrow().is_some())
}
