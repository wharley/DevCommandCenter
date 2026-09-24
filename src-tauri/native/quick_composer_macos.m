#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

void dcc_quick_composer_hide(void);

// The WebView stays owned by Tauri. Its content view is hosted in a real
// non-activating panel, created with public AppKit APIs (no class swapping).
@interface DCCQuickComposerPanel : NSPanel
@end
@implementation DCCQuickComposerPanel
- (BOOL)canBecomeKeyWindow { return YES; }
- (BOOL)canBecomeMainWindow { return NO; }
- (void)cancelOperation:(id)sender { dcc_quick_composer_hide(); }
@end

static DCCQuickComposerPanel *panel;
static NSWindow *host;
static __weak NSWindow *previousKeyWindow;

static WKWebView *findWebView(NSView *view) {
    if ([view isKindOfClass:WKWebView.class]) return (WKWebView *)view;
    for (NSView *child in view.subviews) {
        WKWebView *webView = findWebView(child);
        if (webView) return webView;
    }
    return nil;
}

bool dcc_quick_composer_create(void *window) {
    NSCAssert([NSThread isMainThread], @"Panel creation must run on the main thread");
    if (panel) return true;
    host = (__bridge NSWindow *)window;
    NSView *content = host.contentView;
    if (!content) return false;
    panel = [[DCCQuickComposerPanel alloc]
        initWithContentRect:NSMakeRect(0, 0, 720, 440)
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskFullSizeContentView |
                  NSWindowStyleMaskNonactivatingPanel
        backing:NSBackingStoreBuffered defer:NO];
    panel.title = @"DCC";
    panel.titleVisibility = NSWindowTitleHidden;
    panel.titlebarAppearsTransparent = YES;
    panel.movableByWindowBackground = NO;
    panel.releasedWhenClosed = NO;
    panel.floatingPanel = YES;
    panel.becomesKeyOnlyIfNeeded = NO;
    panel.hidesOnDeactivate = NO;
    panel.level = NSFloatingWindowLevel;
    panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
        NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorTransient;
    host.contentView = [[NSView alloc] initWithFrame:content.frame];
    panel.contentView = content;
    [panel setContentSize:NSMakeSize(720, 440)];
    [host orderOut:nil];
    return true;
}

bool dcc_quick_composer_toggle(void) {
    NSCAssert([NSThread isMainThread], @"Panel presentation must run on the main thread");
    if (!panel) return false;
    if (panel.visible) {
        dcc_quick_composer_hide();
        return false;
    }
    previousKeyWindow = NSApp.keyWindow;
    NSPoint mouse = [NSEvent mouseLocation];
    NSScreen *screen = NSScreen.mainScreen;
    for (NSScreen *candidate in NSScreen.screens) {
        if (NSPointInRect(mouse, candidate.frame)) { screen = candidate; break; }
    }
    NSRect area = screen.visibleFrame;
    NSSize size = panel.frame.size;
    [panel setFrameOrigin:NSMakePoint(NSMidX(area) - size.width / 2,
        NSMidY(area) - size.height / 2)];
    [panel makeKeyAndOrderFront:nil];
    WKWebView *webView = findWebView(panel.contentView);
    if (webView) [panel makeFirstResponder:webView];
    return true;
}

bool dcc_quick_composer_show(void) {
    if (!panel) return false;
    if (!panel.visible) return dcc_quick_composer_toggle();
    [panel makeKeyAndOrderFront:nil];
    WKWebView *webView = findWebView(panel.contentView);
    if (webView) [panel makeFirstResponder:webView];
    return true;
}

void dcc_quick_composer_hide(void) {
    [panel orderOut:nil];
    // Restore DCC's previous key window only when DCC itself is active. A
    // panel opened over another app must leave focus with that application.
    if (NSApp.active && previousKeyWindow.visible) [previousKeyWindow makeKeyWindow];
    previousKeyWindow = nil;
}

void dcc_quick_composer_destroy(void) {
    [panel orderOut:nil];
    if (panel && host) {
        NSView *content = panel.contentView;
        panel.contentView = nil;
        host.contentView = content;
    }
    [panel close];
    panel = nil;
    host = nil;
    previousKeyWindow = nil;
}
