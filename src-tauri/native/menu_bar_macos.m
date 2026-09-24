#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

// Tauri owns the WebView; AppKit owns positioning, dismissal and Spaces.
static NSStatusItem *statusItem;
static NSPopover *popover;
static NSWindow *host;
static void (*visibilityChanged)(bool);

static WKWebView *menuBarWebView(NSView *view) {
    if ([view isKindOfClass:WKWebView.class]) return (WKWebView *)view;
    for (NSView *child in view.subviews) {
        WKWebView *webView = menuBarWebView(child);
        if (webView) return webView;
    }
    return nil;
}

@interface DCCMenuBarController : NSObject <NSPopoverDelegate>
- (void)toggle:(id)sender;
@end
@implementation DCCMenuBarController
- (void)toggle:(id)sender {
    if (popover.shown) { [popover performClose:nil]; return; }
    [popover showRelativeToRect:statusItem.button.bounds
                        ofView:statusItem.button preferredEdge:NSRectEdgeMinY];
    [popover.contentViewController.view.window makeKeyWindow];
}
- (void)popoverDidShow:(NSNotification *)notification {
    WKWebView *webView = menuBarWebView(popover.contentViewController.view);
    if (webView) [webView.window makeFirstResponder:webView];
    if (visibilityChanged) visibilityChanged(true);
}
- (void)popoverDidClose:(NSNotification *)notification {
    if (visibilityChanged) visibilityChanged(false);
}
@end
static DCCMenuBarController *controller;

// Monochrome version of the four flows in icons/app-icon.svg. Template rendering
// gives the symbol native contrast in both menu-bar appearances.
static NSImage *dccMenuBarImage(void) {
    NSImage *image = [NSImage imageWithSize:NSMakeSize(18, 18) flipped:NO
        drawingHandler:^BOOL(NSRect rect) {
        NSAffineTransform *transform = [NSAffineTransform transform];
        [transform translateXBy:1 yBy:17];
        [transform scaleXBy:16.0/552.0 yBy:-16.0/552.0];
        [transform translateXBy:-236 yBy:-236];
        [transform concat];
        [[NSColor blackColor] setStroke];
        NSBezierPath *p = [NSBezierPath bezierPath];
        p.lineWidth =  60;
        p.lineCapStyle = NSLineCapStyleRound;
        [p moveToPoint:NSMakePoint(300,326)]; [p lineToPoint:NSMakePoint(352,326)];
        [p curveToPoint:NSMakePoint(430,432) controlPoint1:NSMakePoint(416,326) controlPoint2:NSMakePoint(430,384)];
        [p curveToPoint:NSMakePoint(500,542) controlPoint1:NSMakePoint(430,480) controlPoint2:NSMakePoint(461,505)];
        [p curveToPoint:NSMakePoint(716,710) controlPoint1:NSMakePoint(590,640) controlPoint2:NSMakePoint(590,710)];
        [p moveToPoint:NSMakePoint(724,326)]; [p lineToPoint:NSMakePoint(672,326)];
        [p curveToPoint:NSMakePoint(594,432) controlPoint1:NSMakePoint(608,326) controlPoint2:NSMakePoint(594,384)];
        [p curveToPoint:NSMakePoint(524,542) controlPoint1:NSMakePoint(594,480) controlPoint2:NSMakePoint(563,505)];
        [p curveToPoint:NSMakePoint(308,710) controlPoint1:NSMakePoint(434,640) controlPoint2:NSMakePoint(434,710)];
        [p moveToPoint:NSMakePoint(512,270)]; [p lineToPoint:NSMakePoint(512,754)];
        [p moveToPoint:NSMakePoint(270,512)]; [p lineToPoint:NSMakePoint(754,512)];
        [p stroke];
        return YES;
    }];
    image.template = YES;
    return image;
}

bool dcc_menu_bar_create(void *window, void (*callback)(bool)) {
    NSCAssert(NSThread.isMainThread, @"Menu bar creation must run on main thread");
    if (statusItem) return true;
    host = (__bridge NSWindow *)window;
    NSView *content = host.contentView;
    if (!content) return false;
    visibilityChanged = callback;
    controller = [DCCMenuBarController new];
    popover = [NSPopover new];
    popover.behavior = NSPopoverBehaviorTransient;
    popover.animates = YES;
    popover.delegate = controller;
    NSViewController *viewController = [NSViewController new];
    host.contentView = [[NSView alloc] initWithFrame:content.frame];
    viewController.view = content;
    popover.contentViewController = viewController;
    popover.contentSize = NSMakeSize(368, 480);
    [host orderOut:nil];
    statusItem = [NSStatusBar.systemStatusBar statusItemWithLength:NSSquareStatusItemLength];
    statusItem.button.image = dccMenuBarImage();
    statusItem.button.toolTip = @"Dev Command Center";
    [statusItem.button setAccessibilityLabel:@"DCC"];
    statusItem.button.target = controller;
    statusItem.button.action = @selector(toggle:);
    return true;
}

bool dcc_menu_bar_visible(void) { return popover.shown; }
void dcc_menu_bar_hide(void) { [popover performClose:nil]; }
void dcc_menu_bar_destroy(void) {
    visibilityChanged = NULL;
    [popover close];
    if (popover && host) {
        NSView *content = popover.contentViewController.view;
        popover.contentViewController.view = [[NSView alloc] init];
        host.contentView = content;
    }
    if (statusItem) [NSStatusBar.systemStatusBar removeStatusItem:statusItem];
    statusItem = nil; popover = nil; controller = nil; host = nil;
}
