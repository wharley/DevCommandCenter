#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdbool.h>

bool dcc_browser_press_key(void *pointer, const char *rawKey) {
    if (!pointer || !rawKey || ![NSThread isMainThread]) return false;
    WKWebView *view = (__bridge WKWebView *)pointer;
    if (![view isKindOfClass:[WKWebView class]] || !view.window || view.hidden || !view.window.visible) return false;
    NSString *key = [NSString stringWithUTF8String:rawKey];
    NSDictionary *keys = @{
        @"Enter": @[@36, @"\r"], @"Tab": @[@48, @"\t"], @"Shift+Tab": @[@48, @"\t"],
        @"Escape": @[@53, @"\x1b"], @"Space": @[@49, @" "],
        @"Backspace": @[@51, @"\x7f"], @"Delete": @[@117, @"\uf728"],
        @"Home": @[@115, @"\uf729"], @"End": @[@119, @"\uf72b"],
        @"ArrowLeft": @[@123, @"\uf702"], @"ArrowRight": @[@124, @"\uf703"],
        @"ArrowDown": @[@125, @"\uf701"], @"ArrowUp": @[@126, @"\uf700"]
    };
    NSArray *entry = keys[key]; if (!entry) return false;
    // Target WebKit's responder directly. No NSApp.sendEvent, CGEventPost,
    // clipboard shortcuts, or routing to any other application/window.
    if (![view.window makeFirstResponder:view]) return false;
    NSEventModifierFlags flags = [key isEqualToString:@"Shift+Tab"] ? NSEventModifierFlagShift : 0;
    if ([key hasPrefix:@"Arrow"] || [key isEqualToString:@"Home"] || [key isEqualToString:@"End"] || [key isEqualToString:@"Delete"]) flags |= NSEventModifierFlagFunction;
    for (NSNumber *type in @[@(NSEventTypeKeyDown), @(NSEventTypeKeyUp)]) {
        NSEvent *event = [NSEvent keyEventWithType:type.unsignedIntegerValue location:NSZeroPoint modifierFlags:flags timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:view.window.windowNumber context:nil characters:entry[1] charactersIgnoringModifiers:entry[1] isARepeat:NO keyCode:[entry[0] unsignedShortValue]];
        if (!event) return false;
        if (type.unsignedIntegerValue == NSEventTypeKeyDown) [view keyDown:event]; else [view keyUp:event];
    }
    return true;
}

bool dcc_browser_click_point(void *pointer, double x, double y) {
    if (!pointer || ![NSThread isMainThread] || !isfinite(x) || !isfinite(y)) return false;
    WKWebView *view = (__bridge WKWebView *)pointer;
    if (![view isKindOfClass:[WKWebView class]] || !view.window || view.hidden || !view.window.visible || x < 0 || y < 0 || x >= view.bounds.size.width || y >= view.bounds.size.height) return false;
    NSPoint local = NSMakePoint(x, view.isFlipped ? y : view.bounds.size.height - y);
    NSPoint windowPoint = [view convertPoint:local toView:nil];
    NSView *target = [view hitTest:[view convertPoint:local toView:view.superview]];
    if (!target || (target != view && ![target isDescendantOf:view])) return false;
    [view.window makeFirstResponder:view];
    for (NSNumber *type in @[@(NSEventTypeLeftMouseDown), @(NSEventTypeLeftMouseUp)]) {
        NSEvent *event = [NSEvent mouseEventWithType:type.unsignedIntegerValue location:windowPoint modifierFlags:0 timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:view.window.windowNumber context:nil eventNumber:0 clickCount:1 pressure:type.unsignedIntegerValue == NSEventTypeLeftMouseDown ? 1.0 : 0.0];
        if (!event) return false;
        if (type.unsignedIntegerValue == NSEventTypeLeftMouseDown) [target mouseDown:event]; else [target mouseUp:event];
    }
    return true;
}

bool dcc_browser_click_fraction(void *pointer, double x, double y) {
    if (!pointer || ![NSThread isMainThread] || !isfinite(x) || !isfinite(y) || x < 0 || x >= 1 || y < 0 || y >= 1) return false;
    WKWebView *view = (__bridge WKWebView *)pointer;
    return dcc_browser_click_point(pointer, x * view.bounds.size.width, y * view.bounds.size.height);
}
