#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

// WKWebView renders its own content; this does not capture other applications
// and does not require macOS Screen Recording or Accessibility permission.
typedef void (*DCCBrowserSnapshotCallback)(uint64_t, const unsigned char *, size_t, const char *);
void dcc_browser_snapshot(void *rawWebView, uint64_t request, DCCBrowserSnapshotCallback callback) {
    WKWebView *webView = (__bridge WKWebView *)rawWebView;
    if (!webView || ![NSThread isMainThread] || webView.bounds.size.width <= 0 || webView.bounds.size.height <= 0) {
        callback(request, NULL, 0, "Browser viewport is unavailable");
        return;
    }
    WKSnapshotConfiguration *configuration = [WKSnapshotConfiguration new];
    configuration.rect = webView.bounds;
    CGFloat scale = MIN(1.0, 2048.0 / MAX(webView.bounds.size.width, webView.bounds.size.height));
    configuration.snapshotWidth = @(webView.bounds.size.width * scale);
    NSSize size = NSMakeSize(MAX(1, floor(webView.bounds.size.width * scale)), MAX(1, floor(webView.bounds.size.height * scale)));
    [webView takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *image, NSError *error) {
        if (error || !image) {
            callback(request, NULL, 0, "Browser snapshot failed");
            return;
        }
        // Render at logical pixels to bound Retina snapshots as well.
        NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:NULL pixelsWide:(NSInteger)size.width pixelsHigh:(NSInteger)size.height bitsPerSample:8 samplesPerPixel:4 hasAlpha:YES isPlanar:NO colorSpaceName:NSDeviceRGBColorSpace bytesPerRow:0 bitsPerPixel:0];
        if (!bitmap) { callback(request, NULL, 0, "Browser snapshot allocation failed"); return; }
        [NSGraphicsContext saveGraphicsState];
        [NSGraphicsContext setCurrentContext:[NSGraphicsContext graphicsContextWithBitmapImageRep:bitmap]];
        [image drawInRect:NSMakeRect(0, 0, size.width, size.height) fromRect:NSZeroRect operation:NSCompositingOperationCopy fraction:1.0];
        [NSGraphicsContext restoreGraphicsState];
        NSData *png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
        if (!png || png.length > 4 * 1024 * 1024) {
            callback(request, NULL, 0, "Browser snapshot exceeds the size limit");
            return;
        }
        callback(request, png.bytes, png.length, NULL);
    }];
}
#include <stdlib.h>
#include <string.h>

// WKDownload asks for a destination synchronously on the AppKit main thread.
// This is intentionally a native save panel rather than a default Downloads
// path, so a remote page can never create a file without a human choice.
char *dcc_browser_choose_download_path(const char *suggested_filename) {
    @autoreleasepool {
        if (![NSThread isMainThread]) {
            return NULL;
        }
        NSSavePanel *panel = [NSSavePanel savePanel];
        panel.canCreateDirectories = YES;
        if (suggested_filename != NULL && suggested_filename[0] != '\0') {
            panel.nameFieldStringValue = [NSString stringWithUTF8String:suggested_filename];
        }
        if ([panel runModal] != NSModalResponseOK || panel.URL.path == nil) {
            return NULL;
        }
        const char *path = panel.URL.path.UTF8String;
        return path == NULL ? NULL : strdup(path);
    }
}

void dcc_browser_free_download_path(char *path) {
    free(path);
}
