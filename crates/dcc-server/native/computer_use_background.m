// Background input for computer use. See computer_use_background.h.
//
// Ported from trycua/cua (MIT), cua-driver/rust/crates/platform-macos/src/
// input/{skylight,mouse,keyboard}.rs: the event fields, the 248-byte focus
// records and the timings below follow that implementation.

#import <AppKit/AppKit.h>
#import <dlfcn.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import "computer_use_background.h"
#include <unistd.h>

typedef void (*DCCPostToPid)(pid_t, CGEventRef);
typedef CGError (*DCCPostRecord)(ProcessSerialNumber *, uint8_t *);
typedef void (*DCCSetWindowLocation)(CGEventRef, CGPoint);
typedef void (*DCCSetIntegerField)(CGEventRef, uint32_t, int64_t);
typedef void (*DCCSetAuthentication)(CGEventRef, id);
typedef OSStatus (*DCCProcessForPid)(pid_t, ProcessSerialNumber *);

static struct {
  DCCPostToPid post;
  DCCPostRecord record;
  DCCSetWindowLocation window_location;
  DCCSetIntegerField set_field;
  DCCSetAuthentication set_authentication;
  DCCProcessForPid process_for_pid;
} sky;

static void load_skylight(void) {
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    (void)dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY | RTLD_GLOBAL);
    sky.post = (DCCPostToPid)dlsym(RTLD_DEFAULT, "SLEventPostToPid");
    sky.record = (DCCPostRecord)dlsym(RTLD_DEFAULT, "SLPSPostEventRecordTo");
    sky.window_location = (DCCSetWindowLocation)dlsym(RTLD_DEFAULT, "CGEventSetWindowLocation");
    sky.set_field = (DCCSetIntegerField)dlsym(RTLD_DEFAULT, "SLEventSetIntegerValueField");
    sky.set_authentication = (DCCSetAuthentication)dlsym(RTLD_DEFAULT, "SLEventSetAuthenticationMessage");
    sky.process_for_pid = (DCCProcessForPid)dlsym(RTLD_DEFAULT, "GetProcessForPID");
  });
}

bool dcc_background_available(void) {
  load_skylight();
  return sky.post && sky.record && sky.process_for_pid;
}

#pragma mark - Focus without raising

static void stamp_window_id(uint8_t *record, uint32_t window_id) {
  record[0x3C] = window_id & 0xFF;
  record[0x3D] = (window_id >> 8) & 0xFF;
  record[0x3E] = (window_id >> 16) & 0xFF;
  record[0x3F] = (window_id >> 24) & 0xFF;
}

// The window server's "this process is now focused" record, posted only to
// the target: the person's front app is never defocused, so their key window
// keeps the keyboard.
static bool focus_without_raise(pid_t pid, uint32_t window_id) {
  ProcessSerialNumber psn = {0, 0};
  if (sky.process_for_pid(pid, &psn) != noErr) return false;
  uint8_t record[0xF8] = {0};
  record[0x04] = 0xF8;
  record[0x08] = 0x0D;
  stamp_window_id(record, window_id);
  record[0x8A] = 0x01;
  return sky.record(&psn, record) == kCGErrorSuccess;
}

static bool make_key_window(pid_t pid, uint32_t window_id) {
  ProcessSerialNumber psn = {0, 0};
  if (sky.process_for_pid(pid, &psn) != noErr) return false;
  bool ok = true;
  for (uint8_t kind = 0x01; kind <= 0x02; ++kind) {
    uint8_t record[0xF8] = {0};
    record[0x04] = 0xF8;
    record[0x08] = kind;
    record[0x3A] = 0x10;
    memset(record + 0x20, 0xFF, 0x10);
    stamp_window_id(record, window_id);
    ok = sky.record(&psn, record) == kCGErrorSuccess && ok;
  }
  return ok;
}

#pragma mark - Pointer events

static void set_field(CGEventRef event, uint32_t field, int64_t value) {
  if (sky.set_field) sky.set_field(event, field, value);
  else CGEventSetIntegerValueField(event, (CGEventField)field, value);
}

// Since macOS 27 the window location an event carries is window-local;
// earlier releases read it in screen coordinates.
static CGPoint window_stamp(CGRect window, CGPoint point) {
  if (NSProcessInfo.processInfo.operatingSystemVersion.majorVersion >= 27) {
    return CGPointMake(point.x - window.origin.x, point.y - window.origin.y);
  }
  return point;
}

static void route_to_window(CGEventRef event, pid_t pid, uint32_t window_id, CGPoint stamp) {
  set_field(event, 40, pid);        // the target process (Chromium filters on it)
  set_field(event, 51, window_id);  // window number
  set_field(event, 91, window_id);  // window under the pointer
  set_field(event, 92, window_id);  // ...that can handle this event
  if (sky.window_location) sky.window_location(event, stamp);
}

typedef struct {
  pid_t pid;
  uint32_t window_id;
  int64_t group;
  CGEventSourceRef source;
} DCCPointerStream;

static bool post_mouse(DCCPointerStream *stream, CGEventType type, CGPoint point, CGPoint stamp, CGMouseButton button, int64_t phase, int64_t click_state, int64_t subtype) {
  CGEventRef event = CGEventCreateMouseEvent(stream->source, type, point, button);
  if (!event) return false;
  set_field(event, 0, phase);
  set_field(event, 1, click_state);
  set_field(event, 3, button);
  set_field(event, 7, subtype);
  set_field(event, 58, stream->group);
  route_to_window(event, stream->pid, stream->window_id, stamp);
  CGEventSetFlags(event, 0);
  sky.post(stream->pid, event);
  CFRelease(event);
  return true;
}

static DCCPointerStream open_stream(pid_t pid, uint32_t window_id) {
  struct timespec now;
  clock_gettime(CLOCK_REALTIME, &now);
  return (DCCPointerStream){pid, window_id, now.tv_nsec, CGEventSourceCreate(kCGEventSourceStateHIDSystemState)};
}

static void close_stream(DCCPointerStream *stream) {
  if (stream->source) CFRelease(stream->source);
}

static const CGPoint DCCOffscreen = {-1, -1};

// An off-window click first: Chromium and Electron accept the real click as
// a user gesture only after it.
static bool post_primer(DCCPointerStream *stream, int64_t subtype) {
  bool ok = post_mouse(stream, kCGEventLeftMouseDown, DCCOffscreen, DCCOffscreen, kCGMouseButtonLeft, 1, 1, subtype);
  usleep(1 * 1000);
  ok = post_mouse(stream, kCGEventLeftMouseUp, DCCOffscreen, DCCOffscreen, kCGMouseButtonLeft, 2, 1, subtype) && ok;
  usleep(100 * 1000);
  return ok;
}

bool dcc_background_click(pid_t pid, uint32_t window_id, CGRect window, CGPoint point, bool right, uint8_t click_count) {
  if (!dcc_background_available() || click_count < 1 || click_count > 2) return false;
  if (!focus_without_raise(pid, window_id)) return false;
  usleep(50 * 1000);
  DCCPointerStream stream = open_stream(pid, window_id);
  CGPoint stamp = window_stamp(window, point);
  CGMouseButton button = right ? kCGMouseButtonRight : kCGMouseButtonLeft;
  CGEventType down = right ? kCGEventRightMouseDown : kCGEventLeftMouseDown;
  CGEventType up = right ? kCGEventRightMouseUp : kCGEventLeftMouseUp;
  // AppKit ignores a synthetic press without a move to that point first.
  bool ok = post_mouse(&stream, kCGEventMouseMoved, point, stamp, kCGMouseButtonLeft, 2, 0, 3);
  usleep(15 * 1000);
  if (!right) ok = post_primer(&stream, 3) && ok;
  for (uint8_t click = 1; ok && click <= click_count; ++click) {
    ok = post_mouse(&stream, down, point, stamp, button, 3, click, 3);
    usleep(1 * 1000);
    ok = post_mouse(&stream, up, point, stamp, button, 3, click, 3) && ok;
    if (click < click_count) usleep(80 * 1000);
  }
  close_stream(&stream);
  return ok;
}

bool dcc_background_scroll(pid_t pid, uint32_t window_id, CGRect window, CGPoint point, int32_t delta_x, int32_t delta_y) {
  if (!dcc_background_available()) return false;
  DCCPointerStream stream = open_stream(pid, window_id);
  CGPoint stamp = window_stamp(window, point);
  // Hover first so AppKit and WebKit track the scroll view under the point.
  bool ok = post_mouse(&stream, kCGEventMouseMoved, point, stamp, kCGMouseButtonLeft, 2, 0, 0);
  usleep(30 * 1000);
  int32_t remaining_x = delta_x, remaining_y = delta_y;
  while (ok && (remaining_x != 0 || remaining_y != 0)) {
    int32_t step_x = MAX(-240, MIN(240, remaining_x)), step_y = MAX(-240, MIN(240, remaining_y));
    CGEventRef event = CGEventCreateScrollWheelEvent2(stream.source, kCGScrollEventUnitPixel, 2, -step_y, -step_x, 0);
    if (!event) { ok = false; break; }
    CGEventSetLocation(event, point);
    CGEventSetIntegerValueField(event, kCGScrollWheelEventIsContinuous, 1);
    route_to_window(event, pid, window_id, stamp);
    sky.post(pid, event);
    CFRelease(event);
    remaining_x -= step_x; remaining_y -= step_y;
    usleep(16 * 1000);
  }
  close_stream(&stream);
  return ok;
}

bool dcc_background_drag(pid_t pid, uint32_t window_id, CGRect window, CGPoint from, CGPoint to) {
  if (!dcc_background_available()) return false;
  if (!focus_without_raise(pid, window_id)) return false;
  usleep(50 * 1000);
  DCCPointerStream stream = open_stream(pid, window_id);
  bool ok = post_mouse(&stream, kCGEventMouseMoved, from, window_stamp(window, from), kCGMouseButtonLeft, 2, 0, 0);
  usleep(15 * 1000);
  ok = post_primer(&stream, 0) && ok;
  ok = ok && post_mouse(&stream, kCGEventLeftMouseDown, from, window_stamp(window, from), kCGMouseButtonLeft, 3, 1, 0);
  usleep(30 * 1000);
  const NSUInteger steps = 16;
  for (NSUInteger step = 1; ok && step <= steps; ++step) {
    double t = (double)step / (double)steps;
    CGPoint point = CGPointMake(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
    ok = post_mouse(&stream, kCGEventLeftMouseDragged, point, window_stamp(window, point), kCGMouseButtonLeft, 3, 1, 0);
    usleep(12 * 1000);
  }
  usleep(50 * 1000);
  // Always release the button, even when an intermediate event failed.
  bool released = post_mouse(&stream, kCGEventLeftMouseUp, to, window_stamp(window, to), kCGMouseButtonLeft, 3, 1, 0);
  close_stream(&stream);
  return ok && released;
}

#pragma mark - Keyboard

bool dcc_background_focus_for_keys(pid_t pid, uint32_t window_id, bool (^is_focused)(void)) {
  if (!dcc_background_available() || !is_focused) return false;
  if (!focus_without_raise(pid, window_id)) return false;
  usleep(120 * 1000);
  (void)make_key_window(pid, window_id);
  for (NSUInteger attempt = 0; attempt < 16; ++attempt) {
    if (is_focused()) return true;
    usleep(25 * 1000);
  }
  return false;
}

// The authentication message is built from the event's own record, which
// SkyLight keeps behind the CGEvent (offsets as cua-driver reads them). The
// factory exists from macOS 15; without it the event goes plain.
static void attach_authentication(CGEventRef event, pid_t pid) {
  if (!sky.set_authentication) return;
  Class factory = NSClassFromString(@"SLSEventAuthenticationMessage");
  SEL selector = NSSelectorFromString(@"messageWithEventRecord:pid:version:");
  if (!factory || ![factory respondsToSelector:selector]) return;
  void *record = NULL;
  const size_t offsets[3] = {24, 32, 16};
  for (size_t index = 0; index < 3 && !record; ++index) {
    record = *(void **)((uint8_t *)event + offsets[index]);
  }
  if (!record) return;
  id message = ((id(*)(id, SEL, void *, int32_t, uint32_t))objc_msgSend)(factory, selector, record, pid, 0);
  if (message) sky.set_authentication(event, message);
}

void dcc_background_post_key(pid_t pid, CGEventRef event) {
  attach_authentication(event, pid);
  sky.post(pid, event);
}
