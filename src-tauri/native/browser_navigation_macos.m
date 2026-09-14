#import <Foundation/Foundation.h>
#import <WebKit/WebKit.h>
#import <objc/runtime.h>

typedef void (*DCCBrowserMainFrameNavigationCallback)(uint64_t, const char *, BOOL);

@interface DCCBrowserMainFrameNavigationObserver : NSObject
@property(nonatomic) uint64_t token;
@property(nonatomic) DCCBrowserMainFrameNavigationCallback callback;
@end

@implementation DCCBrowserMainFrameNavigationObserver
- (void)observeValueForKeyPath:(NSString *)keyPath ofObject:(id)object change:(NSDictionary *)change context:(void *)context {
    if (object == nil || ![object isKindOfClass:WKWebView.class]) return;
    WKWebView *webView = (WKWebView *)object;
    NSString *url = webView.URL.absoluteString;
    if (url.length == 0 || self.callback == NULL) return;
    self.callback(self.token, url.UTF8String, webView.loading);
}
@end

static char DCCBrowserMainFrameNavigationObserverKey;

// WKWebView.URL and loading describe only its top-level document. This avoids
// Wry's policy callback, which is also invoked for iframe navigation actions.
void dcc_browser_track_main_frame_navigation(
    void *rawWebView,
    uint64_t token,
    DCCBrowserMainFrameNavigationCallback callback
) {
    WKWebView *webView = (__bridge WKWebView *)rawWebView;
    if (!webView || ![NSThread isMainThread]) return;

    DCCBrowserMainFrameNavigationObserver *old = objc_getAssociatedObject(webView, &DCCBrowserMainFrameNavigationObserverKey);
    if (old) {
        [webView removeObserver:old forKeyPath:@"URL" context:&DCCBrowserMainFrameNavigationObserverKey];
        [webView removeObserver:old forKeyPath:@"loading" context:&DCCBrowserMainFrameNavigationObserverKey];
    }
    DCCBrowserMainFrameNavigationObserver *observer = [DCCBrowserMainFrameNavigationObserver new];
    observer.token = token;
    observer.callback = callback;
    objc_setAssociatedObject(webView, &DCCBrowserMainFrameNavigationObserverKey, observer, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    [webView addObserver:observer forKeyPath:@"URL" options:NSKeyValueObservingOptionInitial | NSKeyValueObservingOptionNew context:&DCCBrowserMainFrameNavigationObserverKey];
    [webView addObserver:observer forKeyPath:@"loading" options:NSKeyValueObservingOptionInitial | NSKeyValueObservingOptionNew context:&DCCBrowserMainFrameNavigationObserverKey];
}
