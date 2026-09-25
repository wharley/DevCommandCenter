// Native regression test: no window is ever shown and no global input is posted.
// Build/run instructions: docs/BROWSER_WORKBENCH.md.
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdlib.h>
extern bool dcc_browser_press_key(void *, const char *);
extern bool dcc_browser_click_point(void *, double, double);
static NSWindow *window;
static WKWebView *view;
static pid_t foreground;
static void check(BOOL ok, NSString *message) {
    if (!ok) { fprintf(stderr, "FAIL: %s\n", message.UTF8String); exit(1); }
}
@interface TestNavigation : NSObject <WKNavigationDelegate>
@end
@implementation TestNavigation
- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation {
    check(!window.visible, @"window should stay hidden");
    [view evaluateJavaScript:@"document.querySelector('input').focus()" completionHandler:^(id value, NSError *error) {
        check(!error, @"focus field");
        check(dcc_browser_press_key((__bridge void *)view, "Space"), @"send key to hidden parent");
        [view evaluateJavaScript:@"document.querySelector('input').value" completionHandler:^(id value, NSError *error) {
            check(!error && [value isEqual:@" "], @"key reaches page");
            check(dcc_browser_click_point((__bridge void *)view, 40, 70), @"click in hidden parent");
            __block BOOL checking = NO;
            [NSTimer scheduledTimerWithTimeInterval:0.1 repeats:YES block:^(NSTimer *timer) {
                if (checking) return;
                checking = YES;
                [view evaluateJavaScript:@"window.clicked === true" completionHandler:^(id value, NSError *error) {
                check(!error, @"read click result");
                checking = NO;
                if (![value boolValue]) return;
                [timer invalidate];
                check(!window.visible && !window.keyWindow, @"no main window presentation");
                check(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier == foreground, @"foreground app unchanged");
                view.hidden = YES;
                check(!dcc_browser_press_key((__bridge void *)view, "Space"), @"hidden Browser view remains blocked");
                check(!dcc_browser_click_point((__bridge void *)view, 40, 70), @"occluded Browser click remains blocked");
                puts("PASS: hidden-window keyboard/click, no focus change, occluded-view protection");
                exit(0);
            }]; }];
        }];
    }];
}
@end
int main() { @autoreleasepool {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
    foreground = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
    window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 500, 300) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    WKWebViewConfiguration *config = [WKWebViewConfiguration new];
    if (@available(macOS 14.0, *)) config.preferences.inactiveSchedulingPolicy = WKInactiveSchedulingPolicyNone;
    view = [[WKWebView alloc] initWithFrame:NSMakeRect(0, 0, 500, 300) configuration:config];
    window.contentView = view;
    TestNavigation *delegate = [TestNavigation new];
    view.navigationDelegate = delegate;
    [view loadHTMLString:@"<input style='position:absolute;left:10px;top:10px'><button style='position:absolute;left:10px;top:50px;width:100px;height:40px' onclick='window.clicked=true'>Test</button>" baseURL:nil];
    [NSTimer scheduledTimerWithTimeInterval:15 repeats:NO block:^(NSTimer *timer) { check(NO, @"timeout"); }];
    [NSApp run];
} }
