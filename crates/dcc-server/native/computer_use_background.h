#pragma once

#include <ApplicationServices/ApplicationServices.h>
#include <stdbool.h>
#include <stdint.h>

// Background input: pointer and keyboard events delivered to one process and
// one of its windows without moving the system cursor and without raising
// the window or activating the app. It rests on private SkyLight symbols
// resolved at runtime; every function returns false when they are missing,
// so the caller falls back to foreground input and says so.
//
// Technique ported from trycua/cua (MIT), cua-driver platform-macos input.

bool dcc_background_available(void);

// `window` is the target's screen bounds; `point` is in screen coordinates.
bool dcc_background_click(pid_t pid, uint32_t window_id, CGRect window, CGPoint point,
                          bool right, uint8_t click_count);
bool dcc_background_scroll(pid_t pid, uint32_t window_id, CGRect window, CGPoint point,
                           int32_t delta_x, int32_t delta_y);
bool dcc_background_drag(pid_t pid, uint32_t window_id, CGRect window, CGPoint from,
                         CGPoint to);

// Makes the window the app's key window without raising it, then waits until
// `is_focused` confirms it. Keys must never be sent when this returns false.
bool dcc_background_focus_for_keys(pid_t pid, uint32_t window_id, bool (^is_focused)(void));
// Posts one keyboard event to the process (with SkyLight's authentication
// message when the system offers it, which Chromium requires).
void dcc_background_post_key(pid_t pid, CGEventRef event);
