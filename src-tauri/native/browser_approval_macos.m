#import <AppKit/AppKit.h>

// A passive card on the current Space. It never activates DCC or takes the
// keyboard from the user's terminal/editor. Only a deliberate button click acts.
@interface DCCBrowserApprovalPanel : NSPanel
@end
@implementation DCCBrowserApprovalPanel
- (BOOL)canBecomeKeyWindow { return YES; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

@interface DCCBrowserApprovalActions : NSObject
@property(nonatomic, copy) NSString *requestID;
@property(nonatomic) double expiresAt;
@property(nonatomic) void (*callback)(const char *, bool);
- (void)allow:(id)sender;
- (void)deny:(id)sender;
@end

static DCCBrowserApprovalPanel *panel;
static DCCBrowserApprovalActions *actions;
static NSTextField *countdown;
static NSString *remainingFormat;
static NSDictionary *lastPayload;

@implementation DCCBrowserApprovalActions
- (void)decide:(BOOL)allowed {
    if (!self.requestID || NSDate.date.timeIntervalSince1970 * 1000 >= self.expiresAt) return;
    NSString *requestID = self.requestID;
    self.requestID = nil;
    [panel orderOut:nil];
    if (self.callback) self.callback(requestID.UTF8String, allowed);
}
- (void)allow:(id)sender { [self decide:YES]; }
- (void)deny:(id)sender { [self decide:NO]; }
@end

static NSTextField *label(NSString *text, CGFloat size, BOOL bold) {
    NSTextField *field = [NSTextField wrappingLabelWithString:text ?: @""];
    field.font = bold ? [NSFont boldSystemFontOfSize:size] : [NSFont systemFontOfSize:size];
    field.selectable = YES;
    field.translatesAutoresizingMaskIntoConstraints = NO;
    return field;
}

void dcc_browser_approval_update(void *mainWindow, const char *json, void (*callback)(const char *, bool)) {
    NSCAssert(NSThread.isMainThread, @"Approval panel must run on the main thread");
    if (!json) {
        [panel orderOut:nil];
        actions.requestID = nil;
        lastPayload = nil;
        return;
    }
    NSDictionary *payload = [NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
    if (!payload) return;
    NSDictionary *request = payload[@"request"];
    NSDictionary *strings = payload[@"labels"];
    double expires = [request[@"expiresAtMs"] doubleValue];
    NSInteger seconds = (NSInteger)ceil((expires - NSDate.date.timeIntervalSince1970 * 1000) / 1000);
    if (seconds <= 0) { [panel orderOut:nil]; return; }
    if (!panel) {
        panel = [[DCCBrowserApprovalPanel alloc] initWithContentRect:NSMakeRect(0, 0, 440, 380)
            styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskFullSizeContentView | NSWindowStyleMaskNonactivatingPanel
            backing:NSBackingStoreBuffered defer:NO];
        panel.title = @"DCC";
        panel.titleVisibility = NSWindowTitleHidden;
        panel.titlebarAppearsTransparent = YES;
        panel.releasedWhenClosed = NO;
        panel.floatingPanel = YES;
        panel.becomesKeyOnlyIfNeeded = YES;
        panel.hidesOnDeactivate = NO;
        panel.level = NSFloatingWindowLevel;
        panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
            NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorTransient;
        actions = [DCCBrowserApprovalActions new];
    }
    if (![payload isEqual:lastPayload]) {
        lastPayload = payload;
        actions.requestID = request[@"requestId"];
        actions.expiresAt = expires;
        actions.callback = callback;
        remainingFormat = strings[@"remaining"];
        NSView *content = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 440, 380)];
        panel.contentView = content;
        NSStackView *stack = [NSStackView new];
        stack.orientation = NSUserInterfaceLayoutOrientationVertical;
        stack.alignment = NSLayoutAttributeLeading;
        stack.spacing = 10;
        stack.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:stack];
        [NSLayoutConstraint activateConstraints:@[
            [stack.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:20],
            [stack.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-20],
            [stack.topAnchor constraintEqualToAnchor:content.topAnchor constant:24],
            [stack.bottomAnchor constraintEqualToAnchor:content.bottomAnchor constant:-20]
        ]];
        [stack addArrangedSubview:label([@"DCC · " stringByAppendingString:strings[@"title"]], 15, YES)];
        NSTextField *description = label(strings[@"description"], 12, NO);
        description.textColor = NSColor.secondaryLabelColor;
        [stack addArrangedSubview:description];
        // Scroll all request details, including long URLs, instead of hiding
        // the exact destination behind ellipses or growing past the display.
        NSString *details = [NSString stringWithFormat:@"%@\n%@\n\n%@: %@\n%@: %@\n\n%@\n%@",
            strings[@"destination"], request[@"url"], strings[@"conversation"], payload[@"conversation"],
            strings[@"provider"], request[@"providerId"], strings[@"reason"], request[@"reason"]];
        NSScrollView *scroll = [NSScrollView new];
        scroll.hasVerticalScroller = YES;
        scroll.drawsBackground = NO;
        NSTextView *text = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, 400, 180)];
        text.editable = NO;
        text.selectable = YES;
        text.drawsBackground = NO;
        text.font = [NSFont systemFontOfSize:13];
        text.textColor = NSColor.labelColor;
        text.string = details;
        text.verticallyResizable = YES;
        text.horizontallyResizable = NO;
        text.autoresizingMask = NSViewWidthSizable;
        text.textContainer.widthTracksTextView = YES;
        text.textContainer.lineBreakMode = NSLineBreakByCharWrapping;
        scroll.documentView = text;
        [stack addArrangedSubview:scroll];
        [scroll.heightAnchor constraintEqualToConstant:180].active = YES;
        countdown = label(@"", 11, NO);
        countdown.textColor = NSColor.secondaryLabelColor;
        [stack addArrangedSubview:countdown];
        NSButton *deny = [NSButton buttonWithTitle:strings[@"deny"] target:actions action:@selector(deny:)];
        NSButton *allow = [NSButton buttonWithTitle:strings[@"allow"] target:actions action:@selector(allow:)];
        // No default Return key: typing in another app can never grant consent.
        allow.keyEquivalent = @"";
        deny.keyEquivalent = @"";
        NSStackView *buttons = [NSStackView stackViewWithViews:@[deny, allow]];
        buttons.spacing = 12;
        [stack addArrangedSubview:buttons];
        for (NSView *view in stack.arrangedSubviews) {
            [view.widthAnchor constraintEqualToAnchor:stack.widthAnchor].active = YES;
        }
        [panel setContentSize:NSMakeSize(440, stack.fittingSize.height + 44)];
    }
    countdown.stringValue = [remainingFormat stringByReplacingOccurrencesOfString:@"{{time}}" withString:[NSString stringWithFormat:@"%lds", (long)seconds]];
    NSWindow *main = (__bridge NSWindow *)mainWindow;
    if (NSApp.active && main.keyWindow) {
        [panel orderOut:nil];
        return;
    }
    if (!panel.visible && actions.requestID) {
        NSPoint mouse = NSEvent.mouseLocation;
        NSScreen *screen = NSScreen.mainScreen;
        for (NSScreen *candidate in NSScreen.screens) {
            if (NSPointInRect(mouse, candidate.frame)) { screen = candidate; break; }
        }
        NSRect area = screen.visibleFrame;
        [panel setFrameOrigin:NSMakePoint(NSMaxX(area) - panel.frame.size.width - 20,
            NSMaxY(area) - panel.frame.size.height - 20)];
        [panel orderFrontRegardless];
    }
}

void dcc_browser_approval_destroy(void) {
    [panel orderOut:nil];
    [panel close];
    panel = nil;
    actions = nil;
    countdown = nil;
    lastPayload = nil;
}
