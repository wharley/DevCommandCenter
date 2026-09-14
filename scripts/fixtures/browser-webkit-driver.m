// Test-only WKWebView evaluator. Never loaded by the product or exposed to MCP.
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdio.h>
extern void dcc_browser_snapshot(void *, uint64_t, void (*)(uint64_t, const unsigned char *, size_t, const char *));
extern bool dcc_browser_press_key(void *, const char *);
extern bool dcc_browser_click_point(void *, double, double);
extern bool dcc_browser_click_fraction(void *, double, double);
static WKWebView *browser;
static WKWebView *popup;
static NSWindow *window;
static void output(id value) {
    NSData *json = [NSJSONSerialization dataWithJSONObject:value ?: [NSNull null] options:NSJSONWritingFragmentsAllowed error:NULL];
    fwrite(json.bytes, 1, json.length, stdout); fputc('\n', stdout); fflush(stdout);
}
static void snapshot(uint64_t request, const unsigned char *bytes, size_t count, const char *error) {
    output(error ? @{ @"error": @"capture failed" } : @{ @"png": [[NSData dataWithBytes:bytes length:count] base64EncodedStringWithOptions:0], @"bytes": @(count) });
}
@interface Driver : NSObject <NSApplicationDelegate, WKNavigationDelegate, WKUIDelegate>
@end
@implementation Driver
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    WKWebViewConfiguration *config = [WKWebViewConfiguration new];
    // Use an isolated store: fixtures never touch a user's cookies or history.
    config.websiteDataStore = [WKWebsiteDataStore nonPersistentDataStore];
    browser = [[WKWebView alloc] initWithFrame:NSMakeRect(0, 0, 900, 650) configuration:config];
    browser.navigationDelegate = self; browser.UIDelegate = self;
    window = [[NSWindow alloc] initWithContentRect:browser.bounds styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    window.title = @"DCC Browser integration fixture";
    window.contentView = browser; [window makeKeyAndOrderFront:nil];
    NSString *fixtureURL = [[[NSProcessInfo processInfo] environment] objectForKey:@"DCC_BROWSER_POPUP_FIXTURE_URL"];
    if (fixtureURL.length > 0) {
        [browser loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:fixtureURL]]];
    } else {
        NSString *html = @"<!doctype html><title>DCC Browser fixture</title><body style='font:20px system-ui;padding:24px'><h1>Browser fixture</h1><label>Name <input id='name' aria-label='Name'></label><label>Quantity <input id='number' type='number' aria-label='Quantity'></label><div role='button' aria-label='Save' tabindex='0' onclick=\"document.body.dataset.clickTrusted=String(event.isTrusted);document.querySelector('#result').textContent=document.querySelector('#name').value+':'+document.querySelector('#number').value\" style='padding:16px;background:#9ed'>Save</div><p id='result'>Waiting</p><input type='password' value='fixture-only-secret'><div style='height:1500px'></div><p>Bottom</p></body>";
        [browser loadHTMLString:html baseURL:[NSURL URLWithString:@"http://localhost/"]];
    }
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        char *line = NULL; size_t capacity = 0;
        while (getline(&line, &capacity, stdin) > 0) {
            NSData *data = [[NSString stringWithUTF8String:line] dataUsingEncoding:NSUTF8StringEncoding];
            NSDictionary *command = [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL];
            dispatch_async(dispatch_get_main_queue(), ^{
                if (command[@"click"]) {
                    BOOL ok = dcc_browser_click_fraction((__bridge void *)browser, [command[@"click"][@"x"] doubleValue], [command[@"click"][@"y"] doubleValue]);
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{ output(@{ @"ok": @(ok) }); });
                    return;
                }
                if (command[@"key"]) {
                    BOOL ok = dcc_browser_press_key((__bridge void *)browser, [command[@"key"] UTF8String]);
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{ output(@{ @"ok": @(ok) }); });
                    return;
                }
                if (command[@"capture"]) { dcc_browser_snapshot((__bridge void *)browser, 1, snapshot); return; }
                if (command[@"popupProbe"]) {
                    popup = nil;
                    [browser evaluateJavaScript:@"window.open('about:blank', 'dcc-auth')" completionHandler:^(id result, NSError *error) {
                        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 150 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
                            if (!popup) { output(@{ @"error": @"popup was not created" }); return; }
                            [popup evaluateJavaScript:@"JSON.stringify({hasOpener: Boolean(window.opener), sharedCookie: document.cookie.includes('dcc_popup=shared')})" completionHandler:^(id probe, NSError *probeError) {
                                if (probeError || !probe) { output(@{ @"error": @"popup probe failed" }); return; }
                                NSData *probeData = [probe dataUsingEncoding:NSUTF8StringEncoding];
                                NSDictionary *value = [NSJSONSerialization JSONObjectWithData:probeData options:0 error:NULL];
                                BOOL sharedStore = popup.configuration.websiteDataStore == browser.configuration.websiteDataStore;
                                [browser.configuration.websiteDataStore.httpCookieStore getAllCookies:^(NSArray<NSHTTPCookie *> *parentCookies) {
                                [popup.configuration.websiteDataStore.httpCookieStore getAllCookies:^(NSArray<NSHTTPCookie *> *cookies) {
                                    BOOL hasCookie = NO;
                                    for (NSHTTPCookie *cookie in cookies) {
                                        if ([cookie.name isEqualToString:@"dcc_fixture_login"] && [cookie.value isEqualToString:@"yes"]) { hasCookie = YES; break; }
                                    }
                                    BOOL parentHasCookie = NO;
                                    for (NSHTTPCookie *cookie in parentCookies) {
                                        if ([cookie.name isEqualToString:@"dcc_fixture_login"]) { parentHasCookie = YES; break; }
                                    }
                                    NSMutableDictionary *result = [value mutableCopy] ?: [@{} mutableCopy];
                                    // about:blank has an opaque document origin, so document.cookie
                                    // itself is not a valid store-sharing probe. Inspect the supplied
                                    // target configuration's cookie store directly.
                                    result[@"sharedCookie"] = @(sharedStore && hasCookie);
                                    result[@"sameStoreObject"] = @(sharedStore);
                                    result[@"parentCookie"] = @(parentHasCookie);
                                    result[@"popupCookie"] = @(hasCookie);
                                    output(result);
                                }];
                                }];
                            }];
                        });
                    }];
                    return;
                }
                [browser evaluateJavaScript:command[@"script"] completionHandler:^(id result, NSError *error) {
                    output(error ? @{ @"error": error.localizedDescription } : result);
                }];
            });
        }
        free(line); dispatch_async(dispatch_get_main_queue(), ^{ [NSApp terminate:nil]; });
    });
    // A crashed test must never leave its fixture behind indefinitely.
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 45 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{ exit(9); });
}
- (WKWebView *)webView:(WKWebView *)webView createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration forNavigationAction:(WKNavigationAction *)navigationAction windowFeatures:(WKWindowFeatures *)windowFeatures {
    // This is the WebKit contract used by the product popup handler: the
    // supplied target configuration preserves opener. The product explicitly
    // copies the Browser store because WebKit otherwise gives this target a
    // different store.
    configuration.websiteDataStore = browser.configuration.websiteDataStore;
    popup = [[WKWebView alloc] initWithFrame:browser.bounds configuration:configuration];
    return popup;
}
- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation { output(@{ @"ready": @YES }); }
@end
int main(void) { @autoreleasepool { [NSApplication sharedApplication]; [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory]; Driver *driver = [Driver new]; NSApp.delegate = driver; [NSApp run]; } return 0; }
