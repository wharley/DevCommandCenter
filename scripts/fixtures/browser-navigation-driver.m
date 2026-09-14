#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdio.h>

extern void dcc_browser_track_main_frame_navigation(void *, uint64_t,
    void (*)(uint64_t, const char *, bool));
static WKWebView *browser;
static NSWindow *window;
static NSMutableArray *events;
static BOOL announced;

static void output(id value) {
    NSData *data = [NSJSONSerialization dataWithJSONObject:value options:0 error:NULL];
    fwrite(data.bytes, 1, data.length, stdout);
    fputc('\n', stdout);
    fflush(stdout);
}

static void navigation(uint64_t token, const char *url, bool loading) {
    [events addObject:@{@"token": @(token), @"url": url ? @(url) : @"",
                       @"loading": @(loading)}];
}

@interface NavigationDriver : NSObject <NSApplicationDelegate, WKNavigationDelegate>
@end
@implementation NavigationDriver
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    events = [NSMutableArray new];
    WKWebViewConfiguration *config = [WKWebViewConfiguration new];
    config.websiteDataStore = [WKWebsiteDataStore nonPersistentDataStore];
    browser = [[WKWebView alloc] initWithFrame:NSMakeRect(0, 0, 800, 600) configuration:config];
    browser.navigationDelegate = self;
    window = [[NSWindow alloc] initWithContentRect:browser.bounds
        styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    window.title = @"DCC navigation regression fixture";
    window.contentView = browser;
    [window makeKeyAndOrderFront:nil];
    dcc_browser_track_main_frame_navigation((__bridge void *)browser, 17, navigation);
    NSString *target = NSProcessInfo.processInfo.environment[@"DCC_NAVIGATION_FIXTURE_URL"];
    [browser loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:target]]];
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        char *line = NULL;
        size_t capacity = 0;
        while (getline(&line, &capacity, stdin) > 0) {
            NSData *data = [@(line) dataUsingEncoding:NSUTF8StringEncoding];
            NSDictionary *command = [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL];
            dispatch_async(dispatch_get_main_queue(), ^{
                if ([command[@"clear"] boolValue]) [events removeAllObjects];
                if (command[@"navigate"]) {
                    [browser loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:command[@"navigate"]]]];
                }
                if (command[@"token"]) {
                    dcc_browser_track_main_frame_navigation((__bridge void *)browser,
                        [command[@"token"] unsignedLongLongValue], navigation);
                }
                void (^report)(void) = ^{
                    output(@{@"url": browser.URL.absoluteString ?: @"",
                             @"loading": @(browser.loading), @"events": [events copy]});
                };
                if (command[@"script"]) {
                    [browser evaluateJavaScript:command[@"script"] completionHandler:^(id value, NSError *error) {
                        // Full navigation can invalidate the evaluation callback; report the
                        // native URL/events regardless, after the bounded local load settles.
                        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 500 * NSEC_PER_MSEC),
                                       dispatch_get_main_queue(), report);
                    }];
                } else {
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 500 * NSEC_PER_MSEC),
                                   dispatch_get_main_queue(), report);
                }
            });
        }
        free(line);
        dispatch_async(dispatch_get_main_queue(), ^{ [NSApp terminate:nil]; });
    });
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC),
                   dispatch_get_main_queue(), ^{ exit(9); });
}
- (void)webView:(WKWebView *)view didFinishNavigation:(WKNavigation *)navigation {
    if (!announced) {
        announced = YES;
        dispatch_async(dispatch_get_main_queue(), ^{ output(@{@"ready": @YES}); });
    }
}
@end

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        NavigationDriver *driver = [NavigationDriver new];
        NSApp.delegate = driver;
        [NSApp run];
    }
    return 0;
}
