#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <ImageIO/ImageIO.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import "computer_use_macos.h"
#include <unistd.h>

static const size_t DCCMaxCaptureDimension = 4096;
static const size_t DCCMaxCapturePixels = 16 * 1024 * 1024;
static const size_t DCCMaxPngBytes = 4 * 1024 * 1024;
static const NSTimeInterval DCCCaptureTimeout = 2.0;
static const CGFloat DCCGeometryTolerance = 0.01;

API_AVAILABLE(macos(14.0))
@interface DCCCaptureOperation : NSObject {
@public
  dispatch_semaphore_t _semaphore;
  CGImageRef _image;
}
@property(nonatomic, strong, nullable) SCShareableContent *content;
@property(nonatomic, strong, nullable) NSError *error;
@property(nonatomic, strong, nullable) SCContentFilter *filter;
@property(nonatomic, strong, nullable) SCStreamConfiguration *configuration;
- (void)setCapturedImage:(CGImageRef)image;
- (CGImageRef)copyCapturedImage;
@end

@implementation DCCCaptureOperation
- (instancetype)init { self = [super init]; if (self) _semaphore = dispatch_semaphore_create(0); return self; }
- (void)dealloc { if (_image) CGImageRelease(_image); }
- (void)setCapturedImage:(CGImageRef)image { @synchronized (self) { if (_image) CGImageRelease(_image); _image = image ? CGImageRetain(image) : NULL; } }
- (CGImageRef)copyCapturedImage { @synchronized (self) { return _image ? CGImageRetain(_image) : NULL; } }
@end

static char *json_string(id value) {
  NSError *error = nil;
  NSData *data = [NSJSONSerialization dataWithJSONObject:value options:0 error:&error];
  if (!data || error || data.length == 0 || data.length == NSUIntegerMax) return NULL;
  char *result = malloc(data.length + 1);
  if (!result) return NULL;
  memcpy(result, data.bytes, data.length);
  result[data.length] = '\0';
  return result;
}

static bool finite_rect(double x, double y, double width, double height) {
  return isfinite(x) && isfinite(y) && isfinite(width) && isfinite(height) && width > 0 && height > 0;
}

static bool rect_matches(NSDictionary *bounds, double x, double y, double width, double height) {
  if (!bounds || !finite_rect(x, y, width, height)) return false;
  NSNumber *actualX = bounds[@"X"], *actualY = bounds[@"Y"];
  NSNumber *actualWidth = bounds[@"Width"], *actualHeight = bounds[@"Height"];
  if (!actualX || !actualY || !actualWidth || !actualHeight) return false;
  return fabs(actualX.doubleValue - x) <= DCCGeometryTolerance && fabs(actualY.doubleValue - y) <= DCCGeometryTolerance && fabs(actualWidth.doubleValue - width) <= DCCGeometryTolerance && fabs(actualHeight.doubleValue - height) <= DCCGeometryTolerance;
}

// NSWorkspace supplies the running-app inventory; CGWindowList supplies public
// window-server IDs and bounds for the visible windows in those applications.
static NSDictionary<NSNumber *, NSRunningApplication *> *running_regular_apps(void) {
  NSMutableDictionary<NSNumber *, NSRunningApplication *> *apps = [NSMutableDictionary dictionary];
  for (NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
    if (app.processIdentifier <= 0 || app.isTerminated || !app.bundleIdentifier || app.activationPolicy != NSApplicationActivationPolicyRegular) continue;
    apps[@(app.processIdentifier)] = app;
  }
  return apps;
}

static bool usable_window(NSDictionary *window) {
  NSNumber *windowId = window[(id)kCGWindowNumber], *pid = window[(id)kCGWindowOwnerPID];
  NSNumber *layer = window[(id)kCGWindowLayer], *onScreen = window[(id)kCGWindowIsOnscreen], *alpha = window[(id)kCGWindowAlpha];
  NSDictionary *bounds = window[(id)kCGWindowBounds];
  if (!windowId || !pid || !layer || !onScreen || !alpha || !bounds || layer.integerValue != 0 || !onScreen.boolValue || alpha.doubleValue <= 0) return false;
  NSNumber *width = bounds[@"Width"], *height = bounds[@"Height"];
  return width && height && isfinite(width.doubleValue) && isfinite(height.doubleValue) && width.doubleValue >= 2 && height.doubleValue >= 2;
}

// Return an owned copy; entries in a CGWindowList are invalid after releasing
// the list itself.
static NSDictionary *window_for(NSString *bundle_id, int32_t expected_pid, uint32_t window_id) {
  if (!bundle_id || bundle_id.length == 0 || expected_pid <= 0 || window_id == 0) return nil;
  NSRunningApplication *expected_app = running_regular_apps()[@(expected_pid)];
  if (!expected_app || ![expected_app.bundleIdentifier isEqualToString:bundle_id]) return nil;
  CFArrayRef windows = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID);
  if (!windows) return nil;
  NSDictionary *found = nil;
  for (NSDictionary *window in (__bridge NSArray *)windows) {
    NSNumber *record_id = window[(id)kCGWindowNumber], *record_pid = window[(id)kCGWindowOwnerPID];
    if (!usable_window(window) || record_id.unsignedIntValue != window_id || record_pid.intValue != expected_pid) continue;
    found = [window copy];
    break;
  }
  CFRelease(windows);
  return found;
}

static NSDictionary *validated_window(const char *bundle_id_utf8, int32_t pid, uint32_t window_id, double x, double y, double width, double height) {
  if (!bundle_id_utf8 || !finite_rect(x, y, width, height)) return nil;
  NSString *bundle_id = [NSString stringWithUTF8String:bundle_id_utf8];
  NSDictionary *window = window_for(bundle_id, pid, window_id);
  if (!window || !rect_matches(window[(id)kCGWindowBounds], x, y, width, height)) return nil;
  return window;
}

static bool screen_capture_access_is_granted(void) {
  if (@available(macOS 10.15, *)) return CGPreflightScreenCaptureAccess();
  return false;
}

static bool request_screen_capture_access(void) {
  if (@available(macOS 10.15, *)) {
    (void)CGRequestScreenCaptureAccess();
    return true;
  }
  return false;
}

static bool request_accessibility_access(void) {
  NSDictionary *options = @{(__bridge id)kAXTrustedCheckOptionPrompt: @YES};
  // AX returns current trust, rather than whether the prompt was requested.
  // The user-facing Settings page is opened by the frontend before this call.
  (void)AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options);
  return true;
}

static bool request_screen_recording_access(void) {
  return request_screen_capture_access();
}

char *dcc_computer_status_json(void) {
  @autoreleasepool {
    NSOperatingSystemVersion version = NSProcessInfo.processInfo.operatingSystemVersion;
    return json_string(@{@"accessibility": @(AXIsProcessTrusted()), @"screenRecording": @(screen_capture_access_is_granted()), @"majorVersion": @(version.majorVersion)});
  }
}

char *dcc_computer_targets_json(void) {
  @autoreleasepool {
    NSDictionary<NSNumber *, NSRunningApplication *> *apps = running_regular_apps();
    CFArrayRef windows = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID);
    if (!windows) return json_string(@[]);
    NSMutableArray *targets = [NSMutableArray array];
    for (NSDictionary *window in (__bridge NSArray *)windows) {
      if (!usable_window(window)) continue;
      NSNumber *pid = window[(id)kCGWindowOwnerPID];
      NSRunningApplication *app = apps[pid];
      if (!app || !app.bundleIdentifier) continue;
      NSDictionary *bounds = window[(id)kCGWindowBounds];
      [targets addObject:@{@"bundleId": app.bundleIdentifier, @"name": app.localizedName ?: app.bundleIdentifier, @"pid": pid, @"windowId": window[(id)kCGWindowNumber], @"title": window[(id)kCGWindowName] ?: @"", @"x": bounds[@"X"], @"y": bounds[@"Y"], @"width": bounds[@"Width"], @"height": bounds[@"Height"]}];
    }
    CFRelease(windows);
    return json_string(targets);
  }
}

bool dcc_computer_request_access(int kind) {
  @autoreleasepool {
    if (kind == 1) return request_accessibility_access();
    if (kind == 2) return request_screen_recording_access();
    return false;
  }
}

API_AVAILABLE(macos(14.0))
static bool capture_operation_wait(DCCCaptureOperation *operation) {
  return dispatch_semaphore_wait(operation->_semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DCCCaptureTimeout * NSEC_PER_SEC))) == 0;
}

bool dcc_computer_capture_png(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, uint8_t **bytes, size_t *length) {
  @autoreleasepool {
    if (!bytes || !length) return false;
    *bytes = NULL; *length = 0;
    if (!screen_capture_access_is_granted() || !finite_rect(x, y, width, height)) return false;
    if (@available(macOS 14.0, *)) {
      double rounded_width = round(width), rounded_height = round(height);
      if (fabs(width - rounded_width) > DCCGeometryTolerance || fabs(height - rounded_height) > DCCGeometryTolerance || rounded_width < 1 || rounded_height < 1 || rounded_width > DCCMaxCaptureDimension || rounded_height > DCCMaxCaptureDimension || rounded_width * rounded_height > DCCMaxCapturePixels) return false;
      DCCCaptureOperation *operation = [[DCCCaptureOperation alloc] init];
      [SCShareableContent getShareableContentExcludingDesktopWindows:YES onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *content, NSError *error) {
        operation.content = content; operation.error = error; dispatch_semaphore_signal(operation->_semaphore);
      }];
      if (!capture_operation_wait(operation) || operation.error || !operation.content) return false;
      // Re-enumerate after the async query: this is the last target check before capture.
      NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
      if (!fresh) return false;
      SCWindow *selected = nil;
      NSString *expected_bundle_id = [NSString stringWithUTF8String:bundle_id];
      for (SCWindow *candidate in operation.content.windows) {
        if (candidate.windowID == window_id && candidate.isOnScreen && candidate.owningApplication.processID == pid && [candidate.owningApplication.bundleIdentifier isEqualToString:expected_bundle_id] && fabs(candidate.frame.origin.x - x) <= DCCGeometryTolerance && fabs(candidate.frame.origin.y - y) <= DCCGeometryTolerance && fabs(candidate.frame.size.width - width) <= DCCGeometryTolerance && fabs(candidate.frame.size.height - height) <= DCCGeometryTolerance) { selected = candidate; break; }
      }
      if (!selected) return false;
      operation.filter = [[SCContentFilter alloc] initWithDesktopIndependentWindow:selected];
      operation.configuration = [[SCStreamConfiguration alloc] init];
      operation.configuration.width = (size_t)rounded_width;
      operation.configuration.height = (size_t)rounded_height;
      operation.configuration.scalesToFit = YES;
      operation.configuration.preservesAspectRatio = NO;
      operation.configuration.showsCursor = NO;
      operation.configuration.ignoreShadowsSingleWindow = YES;
      operation.configuration.ignoreGlobalClipSingleWindow = YES;
      operation.configuration.captureResolution = SCCaptureResolutionNominal;
      [SCScreenshotManager captureImageWithFilter:operation.filter configuration:operation.configuration completionHandler:^(CGImageRef captured, NSError *error) {
        [operation setCapturedImage:captured]; operation.error = error; dispatch_semaphore_signal(operation->_semaphore);
      }];
      if (!capture_operation_wait(operation) || operation.error) return false;
      CGImageRef image = [operation copyCapturedImage];
      if (!image) return false;
      bool ok = false;
      if (CGImageGetWidth(image) == (size_t)rounded_width && CGImageGetHeight(image) == (size_t)rounded_height) {
        CFMutableDataRef output = CFDataCreateMutable(kCFAllocatorDefault, 0);
        CGImageDestinationRef destination = output ? CGImageDestinationCreateWithData(output, CFSTR("public.png"), 1, NULL) : NULL;
        if (destination) {
          CGImageDestinationAddImage(destination, image, NULL);
          if (CGImageDestinationFinalize(destination)) {
            CFIndex count = CFDataGetLength(output);
            if (count > 0 && (uint64_t)count <= DCCMaxPngBytes) {
              uint8_t *copy = malloc((size_t)count);
              if (copy) { memcpy(copy, CFDataGetBytePtr(output), (size_t)count); *bytes = copy; *length = (size_t)count; ok = true; }
            }
          }
          CFRelease(destination);
        }
        if (output) CFRelease(output);
      }
      CGImageRelease(image);
      return ok;
    }
    return false;
  }
}

static bool window_identity_is_unambiguous(pid_t pid, NSDictionary *target) {
  NSDictionary *bounds = target[(id)kCGWindowBounds];
  NSString *title = target[(id)kCGWindowName] ?: @"";
  CFArrayRef windows = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID);
  if (!windows) return false;
  NSUInteger matches = 0;
  for (NSDictionary *candidate in (__bridge NSArray *)windows) {
    if (!usable_window(candidate) || [candidate[(id)kCGWindowOwnerPID] intValue] != pid) continue;
    NSDictionary *candidate_bounds = candidate[(id)kCGWindowBounds];
    NSString *candidate_title = candidate[(id)kCGWindowName] ?: @"";
    if (rect_matches(candidate_bounds, [bounds[@"X"] doubleValue], [bounds[@"Y"] doubleValue], [bounds[@"Width"] doubleValue], [bounds[@"Height"] doubleValue]) && [candidate_title isEqualToString:title] && ++matches > 1) break;
  }
  CFRelease(windows);
  return matches == 1;
}

static bool target_is_topmost_at_point(pid_t pid, uint32_t window_id, CGPoint point) {
  CFArrayRef windows = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID);
  if (!windows) return false;
  bool target_is_first = false;
  for (NSDictionary *candidate in (__bridge NSArray *)windows) {
    if (!usable_window(candidate) || [candidate[(id)kCGWindowOwnerPID] intValue] != pid) continue;
    NSDictionary *bounds = candidate[(id)kCGWindowBounds];
    CGRect rect = CGRectMake([bounds[@"X"] doubleValue], [bounds[@"Y"] doubleValue], [bounds[@"Width"] doubleValue], [bounds[@"Height"] doubleValue]);
    if (CGRectContainsPoint(rect, point)) {
      target_is_first = [candidate[(id)kCGWindowNumber] unsignedIntValue] == window_id;
      break;
    }
  }
  CFRelease(windows);
  return target_is_first;
}

static bool focused_window_matches(pid_t pid, NSDictionary *target) {
  AXUIElementRef app = AXUIElementCreateApplication(pid); if (!app) return false;
  // Do not allow an unresponsive target process to indefinitely block the
  // calling command while proving the focused window identity.
  AXUIElementSetMessagingTimeout(app, 0.2);
  CFTypeRef focused = NULL;
  AXError focused_result = AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute, &focused);
  CFRelease(app);
  if (focused_result != kAXErrorSuccess || !focused) return false;
  AXUIElementSetMessagingTimeout((AXUIElementRef)focused, 0.2);
  CFTypeRef position = NULL, size = NULL, title = NULL;
  AXError position_result = AXUIElementCopyAttributeValue((AXUIElementRef)focused, kAXPositionAttribute, &position);
  AXError size_result = AXUIElementCopyAttributeValue((AXUIElementRef)focused, kAXSizeAttribute, &size);
  AXError title_result = AXUIElementCopyAttributeValue((AXUIElementRef)focused, kAXTitleAttribute, &title);
  CFRelease(focused);
  if (position_result != kAXErrorSuccess || size_result != kAXErrorSuccess || !position || !size) { if (position) CFRelease(position); if (size) CFRelease(size); if (title) CFRelease(title); return false; }
  CGPoint point; CGSize dimensions;
  bool geometry_ok = CFGetTypeID(position) == AXValueGetTypeID() && CFGetTypeID(size) == AXValueGetTypeID() && AXValueGetType((AXValueRef)position) == kAXValueCGPointType && AXValueGetType((AXValueRef)size) == kAXValueCGSizeType && AXValueGetValue((AXValueRef)position, kAXValueCGPointType, &point) && AXValueGetValue((AXValueRef)size, kAXValueCGSizeType, &dimensions);
  CFRelease(position); CFRelease(size);
  NSString *expected_title = target[(id)kCGWindowName] ?: @"";
  // AX does not expose a CGWindowID. Requiring both public AX geometry and
  // title avoids claiming keyboard delivery to a different focused window in
  // the same process when that identity cannot be established.
  bool title_ok = title_result == kAXErrorSuccess && title && CFGetTypeID(title) == CFStringGetTypeID() && [(__bridge NSString *)title isEqualToString:expected_title];
  if (title) CFRelease(title);
  return geometry_ok && title_ok && rect_matches(target[(id)kCGWindowBounds], point.x, point.y, dimensions.width, dimensions.height) && window_identity_is_unambiguous(pid, target);
}

static bool target_is_active_and_focused(pid_t pid, NSDictionary *target) {
  NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
  return app && app.isActive && focused_window_matches(pid, target);
}

static bool ax_window_matches_target(AXUIElementRef window, pid_t pid, NSDictionary *target) {
  if (!window) return false;
  AXUIElementSetMessagingTimeout(window, 0.2);
  CFTypeRef position = NULL, size = NULL, title = NULL;
  AXError position_result = AXUIElementCopyAttributeValue(window, kAXPositionAttribute, &position);
  AXError size_result = AXUIElementCopyAttributeValue(window, kAXSizeAttribute, &size);
  AXError title_result = AXUIElementCopyAttributeValue(window, kAXTitleAttribute, &title);
  CGPoint point; CGSize dimensions;
  bool geometry_ok = position_result == kAXErrorSuccess && size_result == kAXErrorSuccess && position && size && CFGetTypeID(position) == AXValueGetTypeID() && CFGetTypeID(size) == AXValueGetTypeID() && AXValueGetType((AXValueRef)position) == kAXValueCGPointType && AXValueGetType((AXValueRef)size) == kAXValueCGSizeType && AXValueGetValue((AXValueRef)position, kAXValueCGPointType, &point) && AXValueGetValue((AXValueRef)size, kAXValueCGSizeType, &dimensions);
  bool title_ok = title_result == kAXErrorSuccess && title && CFGetTypeID(title) == CFStringGetTypeID() && [(__bridge NSString *)title isEqualToString:(target[(id)kCGWindowName] ?: @"")];
  if (position) CFRelease(position); if (size) CFRelease(size); if (title) CFRelease(title);
  return geometry_ok && title_ok && rect_matches(target[(id)kCGWindowBounds], point.x, point.y, dimensions.width, dimensions.height) && window_identity_is_unambiguous(pid, target);
}

static AXUIElementRef target_element_at_point(pid_t pid, NSDictionary *target, CGPoint point) {
  AXUIElementRef app = AXUIElementCreateApplication(pid);
  if (!app) return NULL;
  AXUIElementSetMessagingTimeout(app, 0.2);
  AXUIElementRef element = NULL;
  AXError result = AXUIElementCopyElementAtPosition(app, point.x, point.y, &element);
  CFRelease(app);
  if (result != kAXErrorSuccess || !element) return NULL;
  AXUIElementSetMessagingTimeout(element, 0.2);
  CFTypeRef raw_window = NULL;
  AXError window_result = AXUIElementCopyAttributeValue(element, kAXWindowAttribute, &raw_window);
  AXUIElementRef window = window_result == kAXErrorSuccess && raw_window && CFGetTypeID(raw_window) == AXUIElementGetTypeID() ? (AXUIElementRef)raw_window : NULL;
  bool matches = window && ax_window_matches_target(window, pid, target);
  if (raw_window) CFRelease(raw_window);
  if (!matches) { CFRelease(element); return NULL; }
  return element;
}

static bool element_has_action(AXUIElementRef element, CFStringRef action) {
  CFArrayRef actions = NULL;
  AXError result = AXUIElementCopyActionNames(element, &actions);
  bool found = result == kAXErrorSuccess && actions && CFArrayContainsValue(actions, CFRangeMake(0, CFArrayGetCount(actions)), action);
  if (actions) CFRelease(actions);
  return found;
}

static bool perform_press_at_point(pid_t pid, NSDictionary *target, CGPoint point, uint8_t click_count) {
  // AX exposes a semantic press, not the temporal mouse sequence required for
  // a true double-click. Refuse it rather than performing a second action.
  if (click_count != 1) return false;
  AXUIElementRef element = target_element_at_point(pid, target, point);
  if (!element || !element_has_action(element, kAXPressAction)) { if (element) CFRelease(element); return false; }
  bool ok = AXUIElementPerformAction(element, kAXPressAction) == kAXErrorSuccess;
  CFRelease(element);
  return ok;
}

static bool perform_scroll_at_point(pid_t pid, NSDictionary *target, CGPoint point, int32_t delta_x, int32_t delta_y) {
  AXUIElementRef element = target_element_at_point(pid, target, point);
  if (!element) return false;
  // A hit child may not own the scroll action. Walk only within the proven
  // target window and stop after a small bound. The public AX scroll-bar
  // increment/decrement actions provide actual window-scoped scrolling.
  for (NSUInteger depth = 0; depth < 16 && element; ++depth) {
    CFTypeRef raw_vertical = NULL, raw_horizontal = NULL;
    AXError vertical_result = AXUIElementCopyAttributeValue(element, kAXVerticalScrollBarAttribute, &raw_vertical);
    AXError horizontal_result = AXUIElementCopyAttributeValue(element, kAXHorizontalScrollBarAttribute, &raw_horizontal);
    AXUIElementRef vertical = vertical_result == kAXErrorSuccess && raw_vertical && CFGetTypeID(raw_vertical) == AXUIElementGetTypeID() ? (AXUIElementRef)raw_vertical : NULL;
    AXUIElementRef horizontal = horizontal_result == kAXErrorSuccess && raw_horizontal && CFGetTypeID(raw_horizontal) == AXUIElementGetTypeID() ? (AXUIElementRef)raw_horizontal : NULL;
    bool vertical_ok = delta_y == 0 || (vertical && element_has_action(vertical, delta_y > 0 ? kAXIncrementAction : kAXDecrementAction));
    bool horizontal_ok = delta_x == 0 || (horizontal && element_has_action(horizontal, delta_x > 0 ? kAXIncrementAction : kAXDecrementAction));
    if (vertical_ok && horizontal_ok) {
      NSUInteger vertical_steps = MIN((NSUInteger)10, MAX((NSUInteger)1, (NSUInteger)(llabs((long long)delta_y) + 119) / 120));
      NSUInteger horizontal_steps = MIN((NSUInteger)10, MAX((NSUInteger)1, (NSUInteger)(llabs((long long)delta_x) + 119) / 120));
      for (NSUInteger step = 0; vertical_ok && step < vertical_steps && delta_y != 0; ++step) vertical_ok = AXUIElementPerformAction(vertical, delta_y > 0 ? kAXIncrementAction : kAXDecrementAction) == kAXErrorSuccess;
      for (NSUInteger step = 0; horizontal_ok && step < horizontal_steps && delta_x != 0; ++step) horizontal_ok = AXUIElementPerformAction(horizontal, delta_x > 0 ? kAXIncrementAction : kAXDecrementAction) == kAXErrorSuccess;
      if (raw_vertical) CFRelease(raw_vertical); if (raw_horizontal) CFRelease(raw_horizontal); CFRelease(element);
      return vertical_ok && horizontal_ok;
    }
    if (raw_vertical) CFRelease(raw_vertical); if (raw_horizontal) CFRelease(raw_horizontal);
    CFTypeRef parent = NULL;
    AXError parent_result = AXUIElementCopyAttributeValue(element, kAXParentAttribute, &parent);
    CFRelease(element); element = parent_result == kAXErrorSuccess && parent && CFGetTypeID(parent) == AXUIElementGetTypeID() ? (AXUIElementRef)parent : NULL;
    if (parent && !element) CFRelease(parent);
  }
  if (element) CFRelease(element);
  return false;
}

// CGEventPostToPid routes events to a process, not a particular window.  Make
// focus of this exact, freshly enumerated target a prerequisite, otherwise a
// browser with several windows could receive input in the wrong one.
static bool activate_target_window(pid_t pid, const char *bundle_id, uint32_t window_id, double x, double y, double width, double height) {
  NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
  if (fresh && target_is_active_and_focused(pid, fresh)) return true;

  __block bool activated = false;
  void (^activate)(void) = ^{
    NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
    if (app && !app.isTerminated) activated = [app activateWithOptions:NSApplicationActivateIgnoringOtherApps];
  };
  if (NSThread.isMainThread) activate();
  else dispatch_sync(dispatch_get_main_queue(), activate);
  if (!activated) return false;

  // Activation is asynchronous at the window server.  Keep the wait bounded
  // and prove focus again for every attempt; do not send a delayed event.
  for (NSUInteger attempt = 0; attempt < 10; ++attempt) {
    fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
    if (fresh && target_is_active_and_focused(pid, fresh)) return true;
    if (attempt + 1 < 10) usleep(25 * 1000);
  }
  return false;
}

static CGEventRef mouse_event(CGEventType type, CGPoint point, uint32_t window_id, double local_x, double local_y, double window_height, NSInteger click_count) {
  NSEventType appkit_type;
  if (type == kCGEventLeftMouseDown) appkit_type = NSEventTypeLeftMouseDown;
  else if (type == kCGEventLeftMouseUp) appkit_type = NSEventTypeLeftMouseUp;
  else appkit_type = NSEventTypeMouseMoved;
  // CGEventPostToPid does not hit-test a mouse point.  Build a public NSEvent
  // with the target window number and window-local point before copying its
  // CGEvent, so AppKit can associate the event with that window.
  NSEvent *appkit_event = [NSEvent mouseEventWithType:appkit_type
                                              location:NSMakePoint(local_x, window_height - local_y)
                                         modifierFlags:0
                                             timestamp:NSProcessInfo.processInfo.systemUptime
                                          windowNumber:(NSInteger)window_id
                                               context:nil
                                           eventNumber:0
                                            clickCount:click_count
                                              pressure:1.0];
  bool built_from_appkit = appkit_event.CGEvent != nil;
  CGEventRef event = built_from_appkit ? CGEventCreateCopy(appkit_event.CGEvent) : NULL;
  if (!event) event = CGEventCreateMouseEvent(NULL, type, point, kCGMouseButtonLeft);
  if (event) {
    if (!built_from_appkit) CGEventSetLocation(event, point);
    CGEventSetIntegerValueField(event, kCGMouseEventWindowUnderMousePointer, window_id);
    CGEventSetIntegerValueField(event, kCGMouseEventWindowUnderMousePointerThatCanHandleThisEvent, window_id);
  }
  return event;
}

static void post_targeted_mouse_move(pid_t pid, CGPoint point, uint32_t window_id, double local_x, double local_y, double window_height) {
  CGEventRef event = mouse_event(kCGEventMouseMoved, point, window_id, local_x, local_y, window_height, 0);
  if (!event) return;
  CGEventPostToPid(pid, event);
  CFRelease(event);
}

bool dcc_computer_click(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, double click_x, double click_y, uint8_t click_count) {
  @autoreleasepool {
    if (!AXIsProcessTrusted() || (click_count != 1 && click_count != 2) || !isfinite(click_x) || !isfinite(click_y) || click_x < 0 || click_y < 0 || click_x >= width || click_y >= height) return false;
    CGPoint point = CGPointMake(x + click_x, y + click_y);
    if (!activate_target_window(pid, bundle_id, window_id, x, y, width, height)) return false;
    NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
    if (!fresh || !target_is_active_and_focused(pid, fresh) || !target_is_topmost_at_point(pid, window_id, point)) return false;
    return perform_press_at_point(pid, fresh, point, click_count);
  }
}

bool dcc_computer_scroll(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, double scroll_x, double scroll_y, int32_t delta_x, int32_t delta_y) {
  @autoreleasepool {
    if (!AXIsProcessTrusted() || !isfinite(scroll_x) || !isfinite(scroll_y) || scroll_x < 0 || scroll_y < 0 || scroll_x >= width || scroll_y >= height || (delta_x == 0 && delta_y == 0) || llabs((long long)delta_x) > 1000 || llabs((long long)delta_y) > 1000) return false;
    CGPoint point = CGPointMake(x + scroll_x, y + scroll_y);
    if (!activate_target_window(pid, bundle_id, window_id, x, y, width, height)) return false;
    NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
    if (!fresh || !target_is_active_and_focused(pid, fresh) || !target_is_topmost_at_point(pid, window_id, point)) return false;
    return perform_scroll_at_point(pid, fresh, point, delta_x, delta_y);
  }
}

static bool post_unicode_chunk(pid_t pid, const UniChar *characters, UniCharCount count, const char *bundle_id, uint32_t window_id, double x, double y, double width, double height) {
  CGEventRef down = CGEventCreateKeyboardEvent(NULL, 0, true), up = CGEventCreateKeyboardEvent(NULL, 0, false);
  if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return false; }
  CGEventKeyboardSetUnicodeString(down, count, characters); CGEventKeyboardSetUnicodeString(up, count, characters);
  NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
  bool focused = fresh && target_is_active_and_focused(pid, fresh);
  if (focused) { CGEventPostToPid(pid, down); CGEventPostToPid(pid, up); }
  CFRelease(down); CFRelease(up); return focused;
}

bool dcc_computer_type(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, const char *text) {
  @autoreleasepool {
    if (!AXIsProcessTrusted() || !text) return false;
    NSString *value = [NSString stringWithUTF8String:text];
    if (!value || value.length == 0 || value.length > 4000) return false;
    if (!activate_target_window(pid, bundle_id, window_id, x, y, width, height)) return false;
    NSUInteger offset = 0;
    while (offset < value.length) {
      NSUInteger count = MIN((NSUInteger)20, value.length - offset);
      if (count > 1 && offset + count < value.length) {
        unichar last = [value characterAtIndex:offset + count - 1], next = [value characterAtIndex:offset + count];
        if (CFStringIsSurrogateHighCharacter(last) && CFStringIsSurrogateLowCharacter(next)) count--;
      }
      UniChar characters[20]; [value getCharacters:characters range:NSMakeRange(offset, count)];
      if (!post_unicode_chunk(pid, characters, (UniCharCount)count, bundle_id, window_id, x, y, width, height)) return false;
      offset += count;
    }
    return true;
  }
}

static bool supported_key_code(uint16_t key_code) {
  switch (key_code) {
    case 0: case 1: case 2: case 3: case 4: case 5: case 6: case 7: case 8: case 9: case 11: case 12: case 13: case 14: case 15: case 16: case 17: case 18: case 19: case 20: case 21: case 22: case 23: case 25: case 26: case 28: case 29: case 31: case 32: case 34: case 35: case 36: case 37: case 38: case 40: case 45: case 46: case 48: case 49: case 51: case 53: case 96: case 97: case 98: case 99: case 100: case 101: case 103: case 109: case 111: case 117: case 118: case 120: case 122: case 123: case 124: case 125: case 126:
      return true;
    default:
      return false;
  }
}

bool dcc_computer_key(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, uint16_t key_code, uint64_t flags) {
  @autoreleasepool {
    const uint64_t allowed_flags = ((uint64_t)1 << 17) | ((uint64_t)1 << 18) | ((uint64_t)1 << 19) | ((uint64_t)1 << 20);
    if (!AXIsProcessTrusted() || !supported_key_code(key_code) || (flags & ~allowed_flags) != 0) return false;
    if (!activate_target_window(pid, bundle_id, window_id, x, y, width, height)) return false;
    CGEventRef down = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key_code, true), up = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key_code, false);
    if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return false; }
    CGEventSetFlags(down, (CGEventFlags)flags); CGEventSetFlags(up, (CGEventFlags)flags);
    NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
    bool focused = fresh && target_is_active_and_focused(pid, fresh);
    if (focused) { CGEventPostToPid(pid, down); CGEventPostToPid(pid, up); }
    CFRelease(down); CFRelease(up); return focused;
  }
}

void dcc_computer_free_string(char *value) { if (value) free(value); }
void dcc_computer_free_bytes(uint8_t *value) { if (value) free(value); }
