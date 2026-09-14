#import <Foundation/Foundation.h>
#import <WebKit/WebKit.h>
#include <math.h>

typedef struct {
    const char *name, *value, *domain, *path;
    int secure, httpOnly, sameSite;
    int64_t expiresUnixSeconds; // -1 is a session cookie
} DCCSessionCookie;
typedef void (*DCCSessionCookieCallback)(uint64_t, NSUInteger, NSUInteger, const char *);

static BOOL dcc_cookie_domain_matches(NSString *left, NSString *right) {
    // WebKit may normalize away a leading dot while retaining the domain-cookie
    // semantics. Compare the effective domain, not its display spelling.
    return [[left stringByTrimmingCharactersInSet:[NSCharacterSet characterSetWithCharactersInString:@"."]]
        isEqualToString:[right stringByTrimmingCharactersInSet:[NSCharacterSet characterSetWithCharactersInString:@"."]]];
}

static BOOL dcc_cookie_matches(NSHTTPCookie *expected, NSHTTPCookie *actual) {
    if (![expected.name isEqualToString:actual.name] || ![expected.value isEqualToString:actual.value]
        || !dcc_cookie_domain_matches(expected.domain, actual.domain)
        || ![expected.path isEqualToString:actual.path]
        || expected.isSecure != actual.isSecure || expected.isHTTPOnly != actual.isHTTPOnly) return NO;
    if ((expected.expiresDate == nil) != (actual.expiresDate == nil)) return NO;
    if (expected.expiresDate && fabs([expected.expiresDate timeIntervalSinceDate:actual.expiresDate]) > 1.0) return NO;
    if (@available(macOS 10.15, *)) {
        if ((expected.sameSitePolicy == nil) != (actual.sameSitePolicy == nil)
            || (expected.sameSitePolicy && ![expected.sameSitePolicy isEqualToString:actual.sameSitePolicy])) return NO;
    }
    return YES;
}

void dcc_browser_set_session_cookies(void *rawWebView, uint64_t request, const DCCSessionCookie *items, size_t count, DCCSessionCookieCallback callback) {
    WKWebView *webView = (__bridge WKWebView *)rawWebView;
    if (!webView || ![NSThread isMainThread] || !items || count == 0) { callback(request, 0, count, "Browser cookie store is unavailable"); return; }
    NSMutableArray<NSHTTPCookie *> *cookies = [NSMutableArray arrayWithCapacity:count];
    NSUInteger rejected = 0;
    for (size_t index = 0; index < count; index++) {
        const DCCSessionCookie item = items[index];
        NSString *name = item.name ? [NSString stringWithUTF8String:item.name] : nil;
        NSString *value = item.value ? [NSString stringWithUTF8String:item.value] : nil;
        NSString *domain = item.domain ? [NSString stringWithUTF8String:item.domain] : nil;
        NSString *path = item.path ? [NSString stringWithUTF8String:item.path] : nil;
        if (!name || !value || !domain || !path) { rejected++; continue; }
        NSMutableDictionary *properties = [@{ NSHTTPCookieName: name, NSHTTPCookieValue: value, NSHTTPCookieDomain: domain, NSHTTPCookiePath: path } mutableCopy];
        // NSHTTPCookie treats presence of this key as the Secure attribute;
        // supplying @NO still creates a Secure cookie.
        if (item.secure) properties[NSHTTPCookieSecure] = @YES;
        // Foundation exposes `isHTTPOnly` on the resulting cookie but no
        // public construction key. WebKit accepts the RFC attribute spelling.
        if (item.httpOnly) properties[@"HttpOnly"] = @YES;
        if (item.expiresUnixSeconds >= 0) properties[NSHTTPCookieExpires] = [NSDate dateWithTimeIntervalSince1970:item.expiresUnixSeconds];
        if (@available(macOS 10.15, *)) {
            if (item.sameSite == 0) properties[NSHTTPCookieSameSitePolicy] = @"none";
            else if (item.sameSite == 1) properties[NSHTTPCookieSameSitePolicy] = NSHTTPCookieSameSiteLax;
            else if (item.sameSite == 2) properties[NSHTTPCookieSameSitePolicy] = NSHTTPCookieSameSiteStrict;
        }
        NSHTTPCookie *cookie = [NSHTTPCookie cookieWithProperties:properties];
        if (cookie) [cookies addObject:cookie]; else rejected++;
    }
    if (cookies.count == 0) { callback(request, 0, rejected, NULL); return; }
    dispatch_group_t group = dispatch_group_create();
    WKHTTPCookieStore *store = webView.configuration.websiteDataStore.httpCookieStore;
    for (NSHTTPCookie *cookie in cookies) { dispatch_group_enter(group); [store setCookie:cookie completionHandler:^{ dispatch_group_leave(group); }]; }
    dispatch_group_notify(group, dispatch_get_main_queue(), ^{
        [store getAllCookies:^(NSArray<NSHTTPCookie *> *stored) {
            NSUInteger imported = 0;
            for (NSHTTPCookie *expected in cookies) {
                for (NSHTTPCookie *actual in stored) {
                    if (dcc_cookie_matches(expected, actual)) { imported++; break; }
                }
            }
            NSUInteger failed = rejected + (cookies.count - imported);
            // A per-cookie rejection is reported through the counts so the UI
            // can state an accurate partial result. Reserve an error string
            // for a total bridge/store failure only.
            callback(request, imported, failed, NULL);
        }];
    });
}
