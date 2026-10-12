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
    return json_string(@{@"accessibility": @((bool)AXIsProcessTrusted()), @"screenRecording": @(screen_capture_access_is_granted()), @"majorVersion": @(version.majorVersion)});
  }
}

int32_t dcc_appshot_frontmost_pid(void) {
  @autoreleasepool { return NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier; }
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

static char *copy_c_string(NSString *value) {
  const char *utf8 = value.UTF8String;
  return utf8 ? strdup(utf8) : NULL;
}

static CGRect window_rect(NSDictionary *window) {
  NSDictionary *bounds = window[(id)kCGWindowBounds];
  return CGRectMake([bounds[@"X"] doubleValue], [bounds[@"Y"] doubleValue], [bounds[@"Width"] doubleValue], [bounds[@"Height"] doubleValue]);
}

static bool window_is_visible_overlay_candidate(NSDictionary *window, int cursor_level) {
  NSNumber *alpha = window[(id)kCGWindowAlpha], *layer = window[(id)kCGWindowLayer];
  return alpha && layer && alpha.doubleValue > 0 && layer.intValue >= 0 && layer.intValue < cursor_level;
}

// Same-process windows stacked above the target that overlap it: open menus,
// popovers, autocomplete lists. They are part of what the user sees in that
// window, so capture composes them and pointer input may land on them.
static NSArray<NSNumber *> *overlay_window_ids(pid_t pid, uint32_t window_id, CGRect target_rect) {
  NSMutableArray<NSNumber *> *result = [NSMutableArray array];
  CFArrayRef above = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenAboveWindow, window_id);
  if (!above) return result;
  int cursor_level = CGWindowLevelForKey(kCGCursorWindowLevelKey);
  for (NSDictionary *candidate in (__bridge NSArray *)above) {
    if ([candidate[(id)kCGWindowOwnerPID] intValue] != pid || !window_is_visible_overlay_candidate(candidate, cursor_level)) continue;
    if (CGRectIsEmpty(CGRectIntersection(window_rect(candidate), target_rect))) continue;
    NSNumber *number = candidate[(id)kCGWindowNumber];
    if (number) [result addObject:number];
  }
  CFRelease(above);
  return result;
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
      // A single-window filter cannot show the app's own menus or popovers,
      // which are separate windows. When one overlaps the target, compose the
      // target and those windows from the display, cropped to the target.
      CGRect target_rect = CGRectMake(x, y, width, height);
      NSArray<NSNumber *> *overlay_ids = overlay_window_ids(pid, window_id, target_rect);
      SCDisplay *display = nil;
      NSMutableArray<SCWindow *> *included = [NSMutableArray arrayWithObject:selected];
      if (overlay_ids.count > 0) {
        for (SCDisplay *candidate in operation.content.displays) {
          if (CGRectContainsRect(CGDisplayBounds(candidate.displayID), target_rect)) { display = candidate; break; }
        }
        for (SCWindow *candidate in operation.content.windows) {
          if (candidate.owningApplication.processID == pid && [overlay_ids containsObject:@(candidate.windowID)]) [included addObject:candidate];
        }
      }
      operation.configuration = [[SCStreamConfiguration alloc] init];
      if (display && included.count > 1) {
        CGRect display_bounds = CGDisplayBounds(display.displayID);
        operation.filter = [[SCContentFilter alloc] initWithDisplay:display includingWindows:included];
        operation.configuration.sourceRect = CGRectMake(x - display_bounds.origin.x, y - display_bounds.origin.y, width, height);
      } else {
        operation.filter = [[SCContentFilter alloc] initWithDesktopIndependentWindow:selected];
      }
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

#pragma mark - Window identity and focus

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

// AX does not expose a CGWindowID. Requiring both public AX geometry and
// title, unambiguous within the process, avoids acting on a different window
// of the same application.
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

static AXUIElementRef copy_ax_window(pid_t pid, NSDictionary *target) {
  AXUIElementRef app = AXUIElementCreateApplication(pid);
  if (!app) return NULL;
  AXUIElementSetMessagingTimeout(app, 0.5);
  CFTypeRef raw_windows = NULL;
  AXError result = AXUIElementCopyAttributeValue(app, kAXWindowsAttribute, &raw_windows);
  CFRelease(app);
  if (result != kAXErrorSuccess || !raw_windows) return NULL;
  AXUIElementRef found = NULL;
  if (CFGetTypeID(raw_windows) == CFArrayGetTypeID()) {
    for (id candidate in (__bridge NSArray *)raw_windows) {
      AXUIElementRef window = (__bridge AXUIElementRef)candidate;
      if (CFGetTypeID(window) == AXUIElementGetTypeID() && ax_window_matches_target(window, pid, target)) { found = (AXUIElementRef)CFRetain(window); break; }
    }
  }
  CFRelease(raw_windows);
  return found;
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
  bool matches = CFGetTypeID(focused) == AXUIElementGetTypeID() && ax_window_matches_target((AXUIElementRef)focused, pid, target);
  CFRelease(focused);
  return matches;
}

static bool target_is_active_and_focused(pid_t pid, NSDictionary *target) {
  NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
  return app && app.isActive && focused_window_matches(pid, target);
}

// Keyboard and pointer events reach whichever window is frontmost and key, so
// the exact target must be that window first. AX raise/main/frontmost are the
// public controls an accessibility client has over another app's windows and,
// unlike NSRunningApplication activation alone, they work while DCC itself is
// not the active app and they select the right window of a multi-window app.
static int32_t focus_target_window(const char *bundle_id, pid_t pid, uint32_t window_id, double x, double y, double width, double height, NSDictionary **fresh_out) {
  NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
  if (!fresh) return DCC_COMPUTER_ERR_TARGET_UNAVAILABLE;
  if (target_is_active_and_focused(pid, fresh)) { *fresh_out = fresh; return DCC_COMPUTER_OK; }

  AXUIElementRef window = copy_ax_window(pid, fresh);
  if (window) {
    AXUIElementSetMessagingTimeout(window, 0.5);
    (void)AXUIElementPerformAction(window, kAXRaiseAction);
    (void)AXUIElementSetAttributeValue(window, kAXMainAttribute, kCFBooleanTrue);
    CFRelease(window);
  }
  AXUIElementRef app = AXUIElementCreateApplication(pid);
  if (app) {
    AXUIElementSetMessagingTimeout(app, 0.5);
    (void)AXUIElementSetAttributeValue(app, kAXFrontmostAttribute, kCFBooleanTrue);
    CFRelease(app);
  }
  void (^activate)(void) = ^{
    NSRunningApplication *running = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
    if (running && !running.isTerminated && !running.isActive) (void)[running activateWithOptions:NSApplicationActivateIgnoringOtherApps];
  };
  if (NSThread.isMainThread) activate();
  else dispatch_sync(dispatch_get_main_queue(), activate);

  // Activation is asynchronous at the window server. Keep the wait bounded
  // and prove focus again for every attempt; never send a delayed event.
  for (NSUInteger attempt = 0; attempt < 20; ++attempt) {
    usleep(25 * 1000);
    fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
    if (!fresh) return DCC_COMPUTER_ERR_TARGET_UNAVAILABLE;
    if (target_is_active_and_focused(pid, fresh)) { *fresh_out = fresh; return DCC_COMPUTER_OK; }
  }
  return DCC_COMPUTER_ERR_FOCUS;
}

// Window lists include click-through overlays (screen dimmers, recording
// frames) that never receive the event. The accessibility hit test follows the
// window server's own routing, so it confirms whether the target really is
// what the pointer reaches at that point.
static bool accessibility_hit_belongs_to(pid_t pid, CGPoint point) {
  AXUIElementRef system = AXUIElementCreateSystemWide();
  if (!system) return false;
  AXUIElementSetMessagingTimeout(system, 0.25);
  AXUIElementRef element = NULL;
  AXError result = AXUIElementCopyElementAtPosition(system, (float)point.x, (float)point.y, &element);
  CFRelease(system);
  pid_t owner = 0;
  bool belongs = result == kAXErrorSuccess && element && AXUIElementGetPid(element, &owner) == kAXErrorSuccess && owner == pid;
  if (element) CFRelease(element);
  return belongs;
}

// A pointer event posted to the HID stream lands on whatever window is
// frontmost at that point. It must be the target or one of the target app's
// own overlays (which capture composes); anything else is reported by name.
static int32_t pointer_reachability(pid_t pid, uint32_t window_id, CGRect target_rect, CGPoint point, NSString **occluder) {
  CFArrayRef above = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenAboveWindow, window_id);
  if (!above) return DCC_COMPUTER_ERR_TARGET_UNAVAILABLE;
  int32_t result = CGRectContainsPoint(target_rect, point) ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_OUTSIDE_TARGET;
  int cursor_level = CGWindowLevelForKey(kCGCursorWindowLevelKey);
  for (NSDictionary *candidate in (__bridge NSArray *)above) {
    if (!window_is_visible_overlay_candidate(candidate, cursor_level) || !CGRectContainsPoint(window_rect(candidate), point)) continue;
    if ([candidate[(id)kCGWindowOwnerPID] intValue] == pid) {
      result = CGRectIntersectsRect(window_rect(candidate), target_rect) ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_OUTSIDE_TARGET;
    } else if (!accessibility_hit_belongs_to(pid, point)) {
      if (occluder) *occluder = candidate[(id)kCGWindowOwnerName] ?: @"another application";
      result = DCC_COMPUTER_ERR_OCCLUDED;
    }
    break;
  }
  CFRelease(above);
  return result;
}

static int32_t prepare_pointer(const char *bundle_id, pid_t pid, uint32_t window_id, double x, double y, double width, double height, const CGPoint *points, NSUInteger count, char **detail) {
  if (!AXIsProcessTrusted()) return DCC_COMPUTER_ERR_PERMISSION;
  NSDictionary *fresh = nil;
  int32_t status = focus_target_window(bundle_id, pid, window_id, x, y, width, height, &fresh);
  if (status != DCC_COMPUTER_OK) return status;
  CGRect target_rect = CGRectMake(x, y, width, height);
  for (NSUInteger index = 0; index < count; ++index) {
    NSString *occluder = nil;
    status = pointer_reachability(pid, window_id, target_rect, points[index], &occluder);
    if (status != DCC_COMPUTER_OK) {
      if (occluder && detail) *detail = copy_c_string(occluder);
      return status;
    }
  }
  return DCC_COMPUTER_OK;
}

#pragma mark - Pointer input

static bool post_mouse(CGEventSourceRef source, CGEventType type, CGPoint point, CGMouseButton button, int64_t click_state) {
  CGEventRef event = CGEventCreateMouseEvent(source, type, point, button);
  if (!event) return false;
  if (click_state > 0) CGEventSetIntegerValueField(event, kCGMouseEventClickState, click_state);
  CGEventPost(kCGHIDEventTap, event);
  CFRelease(event);
  return true;
}

static bool local_point_is_valid(double local_x, double local_y, double width, double height) {
  return isfinite(local_x) && isfinite(local_y) && local_x >= 0 && local_y >= 0 && local_x < width && local_y < height;
}

// Pointer input is real HID input: the target comes to the front and the
// system cursor moves, exactly as when a person clicks. It is the only path
// that reaches web content and custom-drawn controls that publish no
// accessibility action.
static int32_t pointer_click(const char *bundle_id, pid_t pid, uint32_t window_id, double x, double y, double width, double height, CGPoint point, int32_t button, uint8_t click_count, char **detail) {
  if ((button != DCC_COMPUTER_BUTTON_LEFT && button != DCC_COMPUTER_BUTTON_RIGHT) || click_count < 1 || click_count > 2) return DCC_COMPUTER_ERR_INVALID;
  int32_t status = prepare_pointer(bundle_id, pid, window_id, x, y, width, height, &point, 1, detail);
  if (status != DCC_COMPUTER_OK) return status;
  bool right = button == DCC_COMPUTER_BUTTON_RIGHT;
  CGMouseButton mouse_button = right ? kCGMouseButtonRight : kCGMouseButtonLeft;
  CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
  bool ok = post_mouse(source, kCGEventMouseMoved, point, kCGMouseButtonLeft, 0);
  usleep(40 * 1000);
  for (uint8_t click = 1; ok && click <= click_count; ++click) {
    ok = post_mouse(source, right ? kCGEventRightMouseDown : kCGEventLeftMouseDown, point, mouse_button, click);
    usleep(15 * 1000);
    ok = post_mouse(source, right ? kCGEventRightMouseUp : kCGEventLeftMouseUp, point, mouse_button, click) && ok;
    if (click < click_count) usleep(60 * 1000);
  }
  if (source) CFRelease(source);
  return ok ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_EVENT;
}

static int32_t pointer_scroll(const char *bundle_id, pid_t pid, uint32_t window_id, double x, double y, double width, double height, CGPoint point, int32_t delta_x, int32_t delta_y, char **detail) {
  if ((delta_x == 0 && delta_y == 0) || llabs((long long)delta_x) > 5000 || llabs((long long)delta_y) > 5000) return DCC_COMPUTER_ERR_INVALID;
  int32_t status = prepare_pointer(bundle_id, pid, window_id, x, y, width, height, &point, 1, detail);
  if (status != DCC_COMPUTER_OK) return status;
  CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
  bool ok = post_mouse(source, kCGEventMouseMoved, point, kCGMouseButtonLeft, 0);
  usleep(30 * 1000);
  // Positive deltas reveal content further down/right. Continuous pixel
  // wheel events, in bounded steps, behave like a trackpad gesture.
  int32_t remaining_x = delta_x, remaining_y = delta_y;
  while (ok && (remaining_x != 0 || remaining_y != 0)) {
    int32_t step_x = MAX(-240, MIN(240, remaining_x)), step_y = MAX(-240, MIN(240, remaining_y));
    CGEventRef event = CGEventCreateScrollWheelEvent2(source, kCGScrollEventUnitPixel, 2, -step_y, -step_x, 0);
    if (!event) { ok = false; break; }
    CGEventSetLocation(event, point);
    CGEventSetIntegerValueField(event, kCGScrollWheelEventIsContinuous, 1);
    CGEventPost(kCGHIDEventTap, event);
    CFRelease(event);
    remaining_x -= step_x; remaining_y -= step_y;
    usleep(16 * 1000);
  }
  if (source) CFRelease(source);
  return ok ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_EVENT;
}

int32_t dcc_computer_click_point(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, double click_x, double click_y, int32_t button, uint8_t click_count, char **detail) {
  @autoreleasepool {
    if (!bundle_id || !local_point_is_valid(click_x, click_y, width, height)) return DCC_COMPUTER_ERR_INVALID;
    return pointer_click(bundle_id, pid, window_id, x, y, width, height, CGPointMake(x + click_x, y + click_y), button, click_count, detail);
  }
}

int32_t dcc_computer_scroll_point(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, double scroll_x, double scroll_y, int32_t delta_x, int32_t delta_y, char **detail) {
  @autoreleasepool {
    if (!bundle_id || !local_point_is_valid(scroll_x, scroll_y, width, height)) return DCC_COMPUTER_ERR_INVALID;
    return pointer_scroll(bundle_id, pid, window_id, x, y, width, height, CGPointMake(x + scroll_x, y + scroll_y), delta_x, delta_y, detail);
  }
}

int32_t dcc_computer_drag(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, double from_x, double from_y, double to_x, double to_y, char **detail) {
  @autoreleasepool {
    if (!bundle_id || !local_point_is_valid(from_x, from_y, width, height) || !local_point_is_valid(to_x, to_y, width, height)) return DCC_COMPUTER_ERR_INVALID;
    CGPoint points[2] = {CGPointMake(x + from_x, y + from_y), CGPointMake(x + to_x, y + to_y)};
    int32_t status = prepare_pointer(bundle_id, pid, window_id, x, y, width, height, points, 2, detail);
    if (status != DCC_COMPUTER_OK) return status;
    CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
    bool ok = post_mouse(source, kCGEventMouseMoved, points[0], kCGMouseButtonLeft, 0);
    usleep(40 * 1000);
    ok = ok && post_mouse(source, kCGEventLeftMouseDown, points[0], kCGMouseButtonLeft, 1);
    usleep(80 * 1000);
    // Intermediate drag events let the target recognize a drag gesture and
    // track hover targets on the way, rather than a jump between two points.
    const NSUInteger steps = 16;
    for (NSUInteger step = 1; ok && step <= steps; ++step) {
      double t = (double)step / (double)steps;
      CGPoint point = CGPointMake(points[0].x + (points[1].x - points[0].x) * t, points[0].y + (points[1].y - points[0].y) * t);
      ok = post_mouse(source, kCGEventLeftMouseDragged, point, kCGMouseButtonLeft, 1);
      usleep(12 * 1000);
    }
    usleep(40 * 1000);
    // Always release the button, even when an intermediate event failed.
    bool released = post_mouse(source, kCGEventLeftMouseUp, points[1], kCGMouseButtonLeft, 1);
    if (source) CFRelease(source);
    return ok && released ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_EVENT;
  }
}

#pragma mark - Accessibility element map

// Elements observed by one capture. Refs handed to the agent index into this
// list; the elements themselves stay in this process, so the agent can only
// name what DCC observed in the authorized window.
@interface DCCAXTree : NSObject
@property(nonatomic) pid_t pid;
@property(nonatomic) uint32_t windowId;
@property(nonatomic, strong) NSMutableArray *elements;
@property(nonatomic, strong) NSMutableIndexSet *menuIndexes;
@end
@implementation DCCAXTree
@end

static const NSUInteger DCCMaxRetainedTrees = 32;
static const NSUInteger DCCTreeMaxVisited = 6000;
static const NSUInteger DCCTreeMaxRefs = 800;
static const NSUInteger DCCTreeMaxDepth = 64;
static const NSUInteger DCCTreeMaxTextLength = 48 * 1024;
static const NSUInteger DCCTreeMaxFieldLength = 120;
static const CFAbsoluteTime DCCTreeTimeBudget = 2.0;

static NSMutableDictionary<NSNumber *, DCCAXTree *> *tree_registry(void) {
  static NSMutableDictionary<NSNumber *, DCCAXTree *> *registry;
  static dispatch_once_t once;
  dispatch_once(&once, ^{ registry = [NSMutableDictionary dictionary]; });
  return registry;
}

static NSMutableArray<NSNumber *> *tree_order(void) {
  static NSMutableArray<NSNumber *> *order;
  static dispatch_once_t once;
  dispatch_once(&once, ^{ order = [NSMutableArray array]; });
  return order;
}

static uint64_t register_tree(DCCAXTree *tree) {
  static uint64_t next_handle = 0;
  NSMutableDictionary<NSNumber *, DCCAXTree *> *registry = tree_registry();
  @synchronized (registry) {
    uint64_t handle = ++next_handle;
    registry[@(handle)] = tree;
    NSMutableArray<NSNumber *> *order = tree_order();
    [order addObject:@(handle)];
    while (order.count > DCCMaxRetainedTrees) {
      [registry removeObjectForKey:order.firstObject];
      [order removeObjectAtIndex:0];
    }
    return handle;
  }
}

void dcc_computer_ax_tree_release(uint64_t handle) {
  @autoreleasepool {
    NSMutableDictionary<NSNumber *, DCCAXTree *> *registry = tree_registry();
    @synchronized (registry) {
      [registry removeObjectForKey:@(handle)];
      [tree_order() removeObject:@(handle)];
    }
  }
}

static int32_t copy_tree_element(uint64_t handle, uint32_t index, pid_t pid, uint32_t window_id, AXUIElementRef *element, bool *is_menu) {
  NSMutableDictionary<NSNumber *, DCCAXTree *> *registry = tree_registry();
  @synchronized (registry) {
    DCCAXTree *tree = registry[@(handle)];
    if (!tree || tree.pid != pid || tree.windowId != window_id) return DCC_COMPUTER_ERR_ELEMENT_STALE;
    if (index >= tree.elements.count) return DCC_COMPUTER_ERR_INVALID;
    *element = (AXUIElementRef)CFRetain((__bridge CFTypeRef)tree.elements[index]);
    *is_menu = [tree.menuIndexes containsIndex:index];
  }
  return DCC_COMPUTER_OK;
}

static bool element_has_action(AXUIElementRef element, CFStringRef action) {
  CFArrayRef actions = NULL;
  AXError result = AXUIElementCopyActionNames(element, &actions);
  bool found = result == kAXErrorSuccess && actions && CFArrayContainsValue(actions, CFRangeMake(0, CFArrayGetCount(actions)), action);
  if (actions) CFRelease(actions);
  return found;
}

static bool copy_element_frame(AXUIElementRef element, CGRect *frame) {
  CFTypeRef position = NULL, size = NULL;
  AXError position_result = AXUIElementCopyAttributeValue(element, kAXPositionAttribute, &position);
  AXError size_result = AXUIElementCopyAttributeValue(element, kAXSizeAttribute, &size);
  CGPoint origin; CGSize dimensions;
  bool ok = position_result == kAXErrorSuccess && size_result == kAXErrorSuccess && position && size && CFGetTypeID(position) == AXValueGetTypeID() && CFGetTypeID(size) == AXValueGetTypeID() && AXValueGetValue((AXValueRef)position, kAXValueCGPointType, &origin) && AXValueGetValue((AXValueRef)size, kAXValueCGSizeType, &dimensions) && isfinite(origin.x) && isfinite(origin.y) && isfinite(dimensions.width) && isfinite(dimensions.height);
  if (position) CFRelease(position); if (size) CFRelease(size);
  if (ok) *frame = CGRectMake(origin.x, origin.y, dimensions.width, dimensions.height);
  return ok;
}

// The element must still exist, belong to the target process and, unless it
// lives in an open menu of that process, to the exact target window.
static int32_t validate_element(AXUIElementRef element, pid_t pid, NSDictionary *fresh, bool is_menu) {
  AXUIElementSetMessagingTimeout(element, 0.5);
  pid_t owner = 0;
  if (AXUIElementGetPid(element, &owner) != kAXErrorSuccess || owner != pid) return DCC_COMPUTER_ERR_ELEMENT_STALE;
  CFTypeRef role = NULL;
  AXError role_result = AXUIElementCopyAttributeValue(element, kAXRoleAttribute, &role);
  if (role) CFRelease(role);
  if (role_result != kAXErrorSuccess) return DCC_COMPUTER_ERR_ELEMENT_STALE;
  if (is_menu) return DCC_COMPUTER_OK;
  CFTypeRef raw_window = NULL;
  AXError window_result = AXUIElementCopyAttributeValue(element, kAXWindowAttribute, &raw_window);
  if (window_result == kAXErrorSuccess && raw_window && CFGetTypeID(raw_window) == AXUIElementGetTypeID()) {
    bool matches = ax_window_matches_target((AXUIElementRef)raw_window, pid, fresh);
    CFRelease(raw_window);
    return matches ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_ELEMENT_OUTSIDE;
  }
  if (raw_window) CFRelease(raw_window);
  // Some element kinds do not publish AXWindow. They were observed under the
  // target window, so require that their frame still intersects it.
  CGRect frame;
  return copy_element_frame(element, &frame) && CGRectIntersectsRect(frame, window_rect(fresh)) ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_ELEMENT_OUTSIDE;
}

static int32_t visible_element_center(AXUIElementRef element, NSDictionary *fresh, CGPoint *center) {
  CGRect frame;
  if (!copy_element_frame(element, &frame)) return DCC_COMPUTER_ERR_ELEMENT_OUTSIDE;
  CGRect visible = CGRectIntersection(frame, window_rect(fresh));
  if (CGRectIsNull(visible) || visible.size.width < 1 || visible.size.height < 1) return DCC_COMPUTER_ERR_ELEMENT_OUTSIDE;
  *center = CGPointMake(CGRectGetMidX(visible), CGRectGetMidY(visible));
  return DCC_COMPUTER_OK;
}

typedef int32_t (^DCCElementOperation)(AXUIElementRef element, NSDictionary *fresh, bool is_menu);

static int32_t with_tree_element(const char *bundle_id, pid_t pid, uint32_t window_id, double x, double y, double width, double height, uint64_t handle, uint32_t index, DCCElementOperation operation) {
  if (!AXIsProcessTrusted()) return DCC_COMPUTER_ERR_PERMISSION;
  if (!bundle_id) return DCC_COMPUTER_ERR_INVALID;
  NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
  if (!fresh) return DCC_COMPUTER_ERR_TARGET_UNAVAILABLE;
  AXUIElementRef element = NULL; bool is_menu = false;
  int32_t status = copy_tree_element(handle, index, pid, window_id, &element, &is_menu);
  if (status != DCC_COMPUTER_OK) return status;
  status = validate_element(element, pid, fresh, is_menu);
  if (status == DCC_COMPUTER_OK) status = operation(element, fresh, is_menu);
  CFRelease(element);
  return status;
}

// A ref click prefers the element's own accessibility action: it is exact, does
// not move the user's cursor and works without bringing the app forward. When
// the element publishes no such action (a clickable web div, a double-click,
// a canvas), the same element is clicked with real pointer input at the
// center of its visible part.
int32_t dcc_computer_click_element(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, uint64_t handle, uint32_t index, int32_t button, uint8_t click_count, int32_t *method, char **detail) {
  @autoreleasepool {
    if (!method || (button != DCC_COMPUTER_BUTTON_LEFT && button != DCC_COMPUTER_BUTTON_RIGHT) || click_count < 1 || click_count > 2) return DCC_COMPUTER_ERR_INVALID;
    *method = 0;
    return with_tree_element(bundle_id, pid, window_id, x, y, width, height, handle, index, ^int32_t(AXUIElementRef element, NSDictionary *fresh, bool is_menu) {
      CFStringRef semantic = NULL;
      if (click_count == 1 && button == DCC_COMPUTER_BUTTON_LEFT && element_has_action(element, kAXPressAction)) semantic = kAXPressAction;
      else if (click_count == 1 && button == DCC_COMPUTER_BUTTON_RIGHT && element_has_action(element, kAXShowMenuAction)) semantic = kAXShowMenuAction;
      if (semantic) {
        AXError result = AXUIElementPerformAction(element, semantic);
        if (result == kAXErrorSuccess) { *method = DCC_COMPUTER_METHOD_ACCESSIBILITY; return DCC_COMPUTER_OK; }
        if (result == kAXErrorInvalidUIElement) return DCC_COMPUTER_ERR_ELEMENT_STALE;
      }
      CGPoint center;
      int32_t status = visible_element_center(element, fresh, &center);
      if (status != DCC_COMPUTER_OK) return status;
      *method = DCC_COMPUTER_METHOD_POINTER;
      return pointer_click(bundle_id, pid, window_id, x, y, width, height, center, button, click_count, detail);
    });
  }
}

int32_t dcc_computer_scroll_element(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, uint64_t handle, uint32_t index, int32_t delta_x, int32_t delta_y, char **detail) {
  @autoreleasepool {
    return with_tree_element(bundle_id, pid, window_id, x, y, width, height, handle, index, ^int32_t(AXUIElementRef element, NSDictionary *fresh, bool is_menu) {
      CGPoint center;
      int32_t status = visible_element_center(element, fresh, &center);
      if (status != DCC_COMPUTER_OK) return status;
      return pointer_scroll(bundle_id, pid, window_id, x, y, width, height, center, delta_x, delta_y, detail);
    });
  }
}

int32_t dcc_computer_set_value(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, uint64_t handle, uint32_t index, const char *value, char **detail) {
  @autoreleasepool {
    NSString *string = value ? [NSString stringWithUTF8String:value] : nil;
    if (!string) return DCC_COMPUTER_ERR_INVALID;
    return with_tree_element(bundle_id, pid, window_id, x, y, width, height, handle, index, ^int32_t(AXUIElementRef element, NSDictionary *fresh, bool is_menu) {
      Boolean settable = false;
      if (AXUIElementIsAttributeSettable(element, kAXValueAttribute, &settable) != kAXErrorSuccess || !settable) return DCC_COMPUTER_ERR_UNSUPPORTED;
      // Focus first, as a person would; forms often validate on focus/blur.
      (void)AXUIElementSetAttributeValue(element, kAXFocusedAttribute, kCFBooleanTrue);
      AXError result = AXUIElementSetAttributeValue(element, kAXValueAttribute, (__bridge CFStringRef)string);
      if (result == kAXErrorSuccess) return DCC_COMPUTER_OK;
      return result == kAXErrorInvalidUIElement ? DCC_COMPUTER_ERR_ELEMENT_STALE : DCC_COMPUTER_ERR_EVENT;
    });
  }
}

#pragma mark - Accessibility tree observation

@interface DCCTreeWalk : NSObject
@property(nonatomic) CGRect windowRect;
@property(nonatomic, strong) NSMutableString *text;
@property(nonatomic, strong) DCCAXTree *tree;
@property(nonatomic) NSUInteger visited;
@property(nonatomic) CFAbsoluteTime deadline;
@property(nonatomic) bool truncated;
@property(nonatomic) bool busy;
@end
@implementation DCCTreeWalk
@end

enum {
  DCCAttrRole, DCCAttrSubrole, DCCAttrTitle, DCCAttrDescription, DCCAttrValue, DCCAttrPlaceholder,
  DCCAttrPosition, DCCAttrSize, DCCAttrEnabled, DCCAttrFocused, DCCAttrSelected, DCCAttrExpanded,
  DCCAttrChildren, DCCAttrBusy, DCCAttrLoaded, DCCAttrCount
};

static NSArray *node_attributes(void) {
  static NSArray *attributes;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    attributes = @[(__bridge id)kAXRoleAttribute, (__bridge id)kAXSubroleAttribute, (__bridge id)kAXTitleAttribute, (__bridge id)kAXDescriptionAttribute, (__bridge id)kAXValueAttribute, @"AXPlaceholderValue", (__bridge id)kAXPositionAttribute, (__bridge id)kAXSizeAttribute, (__bridge id)kAXEnabledAttribute, (__bridge id)kAXFocusedAttribute, (__bridge id)kAXSelectedAttribute, (__bridge id)kAXExpandedAttribute, (__bridge id)kAXChildrenAttribute, @"AXElementBusy", @"AXLoaded"];
  });
  return attributes;
}

static NSSet<NSString *> *interactive_roles(void) {
  static NSSet<NSString *> *roles;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    roles = [NSSet setWithArray:@[@"AXButton", @"AXLink", @"AXTextField", @"AXTextArea", @"AXComboBox", @"AXCheckBox", @"AXRadioButton", @"AXPopUpButton", @"AXMenuButton", @"AXSlider", @"AXMenuItem", @"AXMenuBarItem", @"AXDisclosureTriangle", @"AXIncrementor", @"AXColorWell", @"AXDateField", @"AXSwitch", @"AXToggle"]];
  });
  return roles;
}

static NSSet<NSString *> *text_roles(void) {
  static NSSet<NSString *> *roles;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    roles = [NSSet setWithArray:@[@"AXStaticText", @"AXHeading", @"AXImage", @"AXCell", @"AXRow", @"AXTab", @"AXTabGroup", @"AXList", @"AXTable", @"AXOutline", @"AXGroup", @"AXSheet", @"AXDialog", @"AXToolbar", @"AXWebArea"]];
  });
  return roles;
}

static NSDictionary<NSString *, NSString *> *action_names(void) {
  static NSDictionary<NSString *, NSString *> *names;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    names = @{(__bridge NSString *)kAXPressAction: @"press", (__bridge NSString *)kAXShowMenuAction: @"menu", (__bridge NSString *)kAXIncrementAction: @"increment", (__bridge NSString *)kAXDecrementAction: @"decrement", (__bridge NSString *)kAXConfirmAction: @"confirm", (__bridge NSString *)kAXPickAction: @"pick", (__bridge NSString *)kAXCancelAction: @"cancel"};
  });
  return names;
}

static id attribute_at(CFArrayRef values, CFIndex index) {
  if (!values || index >= CFArrayGetCount(values)) return nil;
  CFTypeRef value = CFArrayGetValueAtIndex(values, index);
  if (!value || CFGetTypeID(value) == CFNullGetTypeID()) return nil;
  if (CFGetTypeID(value) == AXValueGetTypeID() && AXValueGetType((AXValueRef)value) == kAXValueAXErrorType) return nil;
  return (__bridge id)value;
}

static bool attribute_bool(CFArrayRef values, CFIndex index, bool *out) {
  id value = attribute_at(values, index);
  if (![value isKindOfClass:NSNumber.class]) return false;
  *out = [(NSNumber *)value boolValue];
  return true;
}

static NSString *clean_text(id value) {
  NSString *text = nil;
  if ([value isKindOfClass:NSString.class]) text = value;
  else if ([value isKindOfClass:NSAttributedString.class]) text = [(NSAttributedString *)value string];
  else if ([value isKindOfClass:NSNumber.class]) text = [(NSNumber *)value stringValue];
  if (text.length == 0) return nil;
  NSArray<NSString *> *parts = [text componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
  NSMutableArray<NSString *> *words = [NSMutableArray arrayWithCapacity:parts.count];
  for (NSString *part in parts) if (part.length > 0) [words addObject:part];
  NSString *collapsed = [words componentsJoinedByString:@" "];
  if (collapsed.length == 0) return nil;
  if (collapsed.length > DCCTreeMaxFieldLength) {
    NSRange range = [collapsed rangeOfComposedCharacterSequencesForRange:NSMakeRange(0, DCCTreeMaxFieldLength)];
    collapsed = [[collapsed substringWithRange:range] stringByAppendingString:@"…"];
  }
  return [collapsed stringByReplacingOccurrencesOfString:@"\"" withString:@"\\\""];
}

static NSString *short_role(NSString *role) {
  if (![role hasPrefix:@"AX"] || role.length <= 2) return role ?: @"unknown";
  NSString *rest = [role substringFromIndex:2];
  return [[[rest substringToIndex:1] lowercaseString] stringByAppendingString:[rest substringFromIndex:1]];
}

static bool walk_should_stop(DCCTreeWalk *walk) {
  if (walk.visited >= DCCTreeMaxVisited || walk.tree.elements.count >= DCCTreeMaxRefs || walk.text.length >= DCCTreeMaxTextLength || CFAbsoluteTimeGetCurrent() > walk.deadline) walk.truncated = true;
  return walk.truncated;
}

static void walk_element(DCCTreeWalk *walk, AXUIElementRef element, NSUInteger depth, NSUInteger indent, bool is_menu, NSString *parent_name) {
  if (depth > DCCTreeMaxDepth || walk_should_stop(walk)) return;
  walk.visited += 1;
  AXUIElementSetMessagingTimeout(element, 0.25);
  CFArrayRef values = NULL;
  if (AXUIElementCopyMultipleAttributeValues(element, (__bridge CFArrayRef)node_attributes(), 0, &values) != kAXErrorSuccess || !values) return;

  NSString *role = attribute_at(values, DCCAttrRole);
  if (![role isKindOfClass:NSString.class]) role = nil;
  NSString *subrole = attribute_at(values, DCCAttrSubrole);
  if (![subrole isKindOfClass:NSString.class]) subrole = nil;
  CGRect frame = CGRectNull;
  id position = attribute_at(values, DCCAttrPosition), size = attribute_at(values, DCCAttrSize);
  CGPoint origin; CGSize dimensions;
  if (position && size && AXValueGetValue((__bridge AXValueRef)position, kAXValueCGPointType, &origin) && AXValueGetValue((__bridge AXValueRef)size, kAXValueCGSizeType, &dimensions) && isfinite(origin.x) && isfinite(origin.y) && isfinite(dimensions.width) && isfinite(dimensions.height)) {
    frame = CGRectMake(origin.x, origin.y, dimensions.width, dimensions.height);
  }
  // Content scrolled out of the window is not something the agent can see or
  // reach; skip it and its subtree. Open menus may extend past the window.
  if (!is_menu && !CGRectIsNull(frame) && frame.size.width > 0 && frame.size.height > 0 && !CGRectIntersectsRect(frame, walk.windowRect)) { CFRelease(values); return; }
  if ([role isEqualToString:@"AXWebArea"]) {
    bool busy = false, loaded = true;
    if ((attribute_bool(values, DCCAttrBusy, &busy) && busy) || (attribute_bool(values, DCCAttrLoaded, &loaded) && !loaded)) walk.busy = true;
  }

  bool is_static_text = [role isEqualToString:@"AXStaticText"];
  bool secure = [subrole isEqualToString:@"AXSecureTextField"];
  NSString *name = clean_text(attribute_at(values, DCCAttrTitle)) ?: clean_text(attribute_at(values, DCCAttrDescription));
  if (!name && is_static_text) name = clean_text(attribute_at(values, DCCAttrValue));
  NSString *placeholder = clean_text(attribute_at(values, DCCAttrPlaceholder));
  NSString *value = (is_static_text || secure) ? nil : clean_text(attribute_at(values, DCCAttrValue));
  if (value && name && [value isEqualToString:name]) value = nil;

  NSMutableArray<NSString *> *actions = [NSMutableArray array];
  CFArrayRef raw_actions = NULL;
  // Text runs are by far the most numerous web nodes and are clicked through
  // their parent; skipping their action query keeps large pages in budget.
  if (!is_static_text && AXUIElementCopyActionNames(element, &raw_actions) == kAXErrorSuccess && raw_actions) {
    for (NSString *action in (__bridge NSArray *)raw_actions) {
      NSString *short_name = action_names()[action];
      if (short_name && ![actions containsObject:short_name]) [actions addObject:short_name];
    }
    CFRelease(raw_actions);
  }
  bool editable = false;
  if ([role isEqualToString:@"AXTextField"] || [role isEqualToString:@"AXTextArea"] || [role isEqualToString:@"AXComboBox"]) {
    Boolean settable = false;
    editable = AXUIElementIsAttributeSettable(element, kAXValueAttribute, &settable) == kAXErrorSuccess && settable;
  }
  bool duplicate_text = is_static_text && name && parent_name && [name isEqualToString:parent_name];
  bool interesting = !duplicate_text && (actions.count > 0 || editable || [interactive_roles() containsObject:role] || (name && [text_roles() containsObject:role]));

  NSUInteger child_indent = indent;
  NSString *child_parent_name = parent_name;
  if (interesting) {
    [walk.tree.elements addObject:(__bridge id)element];
    NSUInteger ref = walk.tree.elements.count;
    if (is_menu) [walk.tree.menuIndexes addIndex:ref - 1];
    NSMutableString *line = [NSMutableString string];
    for (NSUInteger level = 0; level < indent; ++level) [line appendString:@"  "];
    [line appendFormat:@"e%lu %@", (unsigned long)ref, short_role(role)];
    if (secure) [line appendString:@" (secure)"];
    else if ([subrole isEqualToString:@"AXSearchField"]) [line appendString:@" (search)"];
    if (name) [line appendFormat:@" \"%@\"", name];
    if (placeholder && !value) [line appendFormat:@" placeholder=\"%@\"", placeholder];
    if ([role isEqualToString:@"AXCheckBox"] || [role isEqualToString:@"AXRadioButton"] || [role isEqualToString:@"AXSwitch"]) {
      id checked = attribute_at(values, DCCAttrValue);
      if ([checked isKindOfClass:NSNumber.class]) [line appendString:[(NSNumber *)checked intValue] == 0 ? @" unchecked" : @" checked"];
    } else if (value) {
      [line appendFormat:@" value=\"%@\"", value];
    }
    bool flag = false;
    if (attribute_bool(values, DCCAttrFocused, &flag) && flag) [line appendString:@" focused"];
    if (attribute_bool(values, DCCAttrEnabled, &flag) && !flag) [line appendString:@" disabled"];
    if (attribute_bool(values, DCCAttrSelected, &flag) && flag) [line appendString:@" selected"];
    if (attribute_bool(values, DCCAttrExpanded, &flag)) [line appendString:flag ? @" expanded" : @" collapsed"];
    if (editable) [line appendString:@" editable"];
    if (actions.count > 0) [line appendFormat:@" [%@]", [actions componentsJoinedByString:@","]];
    if (!CGRectIsNull(frame)) {
      CGRect local = CGRectOffset(frame, -walk.windowRect.origin.x, -walk.windowRect.origin.y);
      [line appendFormat:@" @%ld,%ld %ldx%ld", lround(local.origin.x), lround(local.origin.y), lround(local.size.width), lround(local.size.height)];
    }
    [line appendString:@"\n"];
    [walk.text appendString:line];
    child_indent = indent + 1;
    child_parent_name = name;
  }
  id raw_children = attribute_at(values, DCCAttrChildren);
  NSArray *children = [raw_children isKindOfClass:NSArray.class] ? raw_children : nil;
  CFRelease(values);
  for (id child in children) {
    if (CFGetTypeID((__bridge CFTypeRef)child) != AXUIElementGetTypeID()) continue;
    walk_element(walk, (__bridge AXUIElementRef)child, depth + 1, child_indent, is_menu, child_parent_name);
    if (walk.truncated) break;
  }
}

// Observes the authorized window's accessibility elements as compact text
// lines, `e<ref> role "name" … @x,y wxh` in window-local pixels matching the
// screenshot. Open context menus of the same app are listed after the window.
char *dcc_computer_ax_tree_json(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height) {
  @autoreleasepool {
    if (!AXIsProcessTrusted()) return json_string(@{@"status": @(DCC_COMPUTER_ERR_PERMISSION)});
    NSDictionary *fresh = bundle_id ? validated_window(bundle_id, pid, window_id, x, y, width, height) : nil;
    if (!fresh) return json_string(@{@"status": @(DCC_COMPUTER_ERR_TARGET_UNAVAILABLE)});
    AXUIElementRef window = copy_ax_window(pid, fresh);
    if (!window) return json_string(@{@"status": @(DCC_COMPUTER_OK), @"available": @NO});
    DCCTreeWalk *walk = [[DCCTreeWalk alloc] init];
    walk.windowRect = window_rect(fresh);
    walk.text = [NSMutableString string];
    walk.tree = [[DCCAXTree alloc] init];
    walk.tree.pid = pid;
    walk.tree.windowId = window_id;
    walk.tree.elements = [NSMutableArray array];
    walk.tree.menuIndexes = [NSMutableIndexSet indexSet];
    walk.deadline = CFAbsoluteTimeGetCurrent() + DCCTreeTimeBudget;
    walk_element(walk, window, 0, 0, false, nil);
    CFRelease(window);

    AXUIElementRef app = AXUIElementCreateApplication(pid);
    if (app && !walk.truncated) {
      AXUIElementSetMessagingTimeout(app, 0.25);
      CFTypeRef raw_children = NULL;
      if (AXUIElementCopyAttributeValue(app, kAXChildrenAttribute, &raw_children) == kAXErrorSuccess && raw_children && CFGetTypeID(raw_children) == CFArrayGetTypeID()) {
        for (id child in (__bridge NSArray *)raw_children) {
          AXUIElementRef menu = (__bridge AXUIElementRef)child;
          if (CFGetTypeID(menu) != AXUIElementGetTypeID()) continue;
          CFTypeRef role = NULL;
          bool is_menu = AXUIElementCopyAttributeValue(menu, kAXRoleAttribute, &role) == kAXErrorSuccess && role && CFGetTypeID(role) == CFStringGetTypeID() && CFEqual(role, kAXMenuRole);
          if (role) CFRelease(role);
          if (!is_menu) continue;
          [walk.text appendString:@"open menu:\n"];
          walk_element(walk, menu, 0, 1, true, nil);
          if (walk.truncated) break;
        }
      }
      if (raw_children) CFRelease(raw_children);
    }
    if (app) CFRelease(app);

    uint64_t handle = walk.tree.elements.count > 0 ? register_tree(walk.tree) : 0;
    return json_string(@{@"status": @(DCC_COMPUTER_OK), @"available": @YES, @"handle": @(handle), @"count": @(walk.tree.elements.count), @"text": walk.text, @"truncated": @(walk.truncated), @"busy": @(walk.busy)});
  }
}

// Chromium and Electron build the accessibility tree of web content only for
// assistive technology. AXManualAccessibility is the attribute they document
// for that purpose; unlike AXEnhancedUserInterface it does not change window
// behaviour. The caller restores it when its grant ends.
int32_t dcc_computer_web_accessibility(int32_t pid, bool enabled) {
  @autoreleasepool {
    if (!AXIsProcessTrusted() || pid <= 0) return DCC_COMPUTER_WEB_AX_UNSUPPORTED;
    AXUIElementRef app = AXUIElementCreateApplication(pid);
    if (!app) return DCC_COMPUTER_WEB_AX_UNSUPPORTED;
    AXUIElementSetMessagingTimeout(app, 0.5);
    CFStringRef attribute = CFSTR("AXManualAccessibility");
    if (enabled) {
      CFTypeRef current = NULL;
      bool already = AXUIElementCopyAttributeValue(app, attribute, &current) == kAXErrorSuccess && current && CFGetTypeID(current) == CFBooleanGetTypeID() && CFBooleanGetValue((CFBooleanRef)current);
      if (current) CFRelease(current);
      if (already) { CFRelease(app); return DCC_COMPUTER_WEB_AX_ALREADY_ENABLED; }
    }
    AXError result = AXUIElementSetAttributeValue(app, attribute, enabled ? kCFBooleanTrue : kCFBooleanFalse);
    CFRelease(app);
    return result == kAXErrorSuccess ? DCC_COMPUTER_WEB_AX_CHANGED : DCC_COMPUTER_WEB_AX_UNSUPPORTED;
  }
}

#pragma mark - Keyboard input

static int32_t post_unicode_chunk(pid_t pid, const UniChar *characters, UniCharCount count, const char *bundle_id, uint32_t window_id, double x, double y, double width, double height) {
  NSDictionary *fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
  if (!fresh) return DCC_COMPUTER_ERR_TARGET_UNAVAILABLE;
  if (!target_is_active_and_focused(pid, fresh)) return DCC_COMPUTER_ERR_FOCUS;
  CGEventRef down = CGEventCreateKeyboardEvent(NULL, 0, true), up = CGEventCreateKeyboardEvent(NULL, 0, false);
  if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return DCC_COMPUTER_ERR_EVENT; }
  CGEventKeyboardSetUnicodeString(down, count, characters); CGEventKeyboardSetUnicodeString(up, count, characters);
  // Keyboard events go to the process, never to whichever app is frontmost
  // later: a focus change mid-text cannot leak keystrokes elsewhere.
  CGEventPostToPid(pid, down); CGEventPostToPid(pid, up);
  CFRelease(down); CFRelease(up);
  return DCC_COMPUTER_OK;
}

int32_t dcc_computer_type(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, const char *text) {
  @autoreleasepool {
    if (!AXIsProcessTrusted()) return DCC_COMPUTER_ERR_PERMISSION;
    if (!bundle_id || !text) return DCC_COMPUTER_ERR_INVALID;
    NSString *value = [NSString stringWithUTF8String:text];
    if (!value || value.length == 0 || value.length > 4000) return DCC_COMPUTER_ERR_INVALID;
    NSDictionary *fresh = nil;
    int32_t status = focus_target_window(bundle_id, pid, window_id, x, y, width, height, &fresh);
    if (status != DCC_COMPUTER_OK) return status;
    NSUInteger offset = 0;
    while (offset < value.length) {
      NSUInteger count = MIN((NSUInteger)20, value.length - offset);
      if (count > 1 && offset + count < value.length) {
        unichar last = [value characterAtIndex:offset + count - 1], next = [value characterAtIndex:offset + count];
        if (CFStringIsSurrogateHighCharacter(last) && CFStringIsSurrogateLowCharacter(next)) count--;
      }
      UniChar characters[20]; [value getCharacters:characters range:NSMakeRange(offset, count)];
      status = post_unicode_chunk(pid, characters, (UniCharCount)count, bundle_id, window_id, x, y, width, height);
      if (status != DCC_COMPUTER_OK) return status;
      offset += count;
    }
    return DCC_COMPUTER_OK;
  }
}

static bool supported_key_code(uint16_t key_code) {
  switch (key_code) {
    case 0: case 1: case 2: case 3: case 4: case 5: case 6: case 7: case 8: case 9: case 11: case 12: case 13: case 14: case 15: case 16: case 17: case 18: case 19: case 20: case 21: case 22: case 23: case 25: case 26: case 28: case 29: case 31: case 32: case 34: case 35: case 36: case 37: case 38: case 40: case 45: case 46: case 48: case 49: case 51: case 53: case 96: case 97: case 98: case 99: case 100: case 101: case 103: case 109: case 111: case 115: case 116: case 117: case 118: case 119: case 120: case 121: case 122: case 123: case 124: case 125: case 126:
      return true;
    default:
      return false;
  }
}

int32_t dcc_computer_key(const char *bundle_id, int32_t pid, uint32_t window_id, double x, double y, double width, double height, uint16_t key_code, uint64_t flags) {
  @autoreleasepool {
    const uint64_t allowed_flags = ((uint64_t)1 << 17) | ((uint64_t)1 << 18) | ((uint64_t)1 << 19) | ((uint64_t)1 << 20);
    if (!AXIsProcessTrusted()) return DCC_COMPUTER_ERR_PERMISSION;
    if (!bundle_id || !supported_key_code(key_code) || (flags & ~allowed_flags) != 0) return DCC_COMPUTER_ERR_INVALID;
    NSDictionary *fresh = nil;
    int32_t status = focus_target_window(bundle_id, pid, window_id, x, y, width, height, &fresh);
    if (status != DCC_COMPUTER_OK) return status;
    CGEventRef down = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key_code, true), up = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key_code, false);
    if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return DCC_COMPUTER_ERR_EVENT; }
    CGEventSetFlags(down, (CGEventFlags)flags); CGEventSetFlags(up, (CGEventFlags)flags);
    fresh = validated_window(bundle_id, pid, window_id, x, y, width, height);
    status = !fresh ? DCC_COMPUTER_ERR_TARGET_UNAVAILABLE : (target_is_active_and_focused(pid, fresh) ? DCC_COMPUTER_OK : DCC_COMPUTER_ERR_FOCUS);
    if (status == DCC_COMPUTER_OK) { CGEventPostToPid(pid, down); CGEventPostToPid(pid, up); }
    CFRelease(down); CFRelease(up);
    return status;
  }
}

void dcc_computer_free_string(char *value) { if (value) free(value); }
void dcc_computer_free_bytes(uint8_t *value) { if (value) free(value); }
