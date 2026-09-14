#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// Every returned string/buffer is allocated by this bridge and must be freed
// with the matching dcc_computer_free_* function. The Rust caller never owns
// a Cocoa object across the FFI boundary.
char *dcc_computer_status_json(void);
char *dcc_computer_targets_json(void);
int32_t dcc_appshot_frontmost_pid(void);
// Returns whether the native permission request was initiated. It does not
// indicate whether macOS has granted the permission; callers must re-check.
bool dcc_computer_request_access(int kind);
// Effects carry the complete target snapshot.  The bridge freshly enumerates
// and compares every field immediately before it captures or posts an event.
bool dcc_computer_capture_png(const char *bundle_id, int32_t pid, uint32_t window_id,
                              double x, double y, double width, double height,
                              uint8_t **bytes, size_t *length);
bool dcc_computer_click(const char *bundle_id, int32_t pid, uint32_t window_id,
                        double x, double y, double width, double height,
                        double click_x, double click_y, uint8_t click_count);
bool dcc_computer_scroll(const char *bundle_id, int32_t pid, uint32_t window_id,
                         double x, double y, double width, double height,
                         double scroll_x, double scroll_y,
                         int32_t delta_x, int32_t delta_y);
bool dcc_computer_type(const char *bundle_id, int32_t pid, uint32_t window_id,
                       double x, double y, double width, double height,
                       const char *text);
bool dcc_computer_key(const char *bundle_id, int32_t pid, uint32_t window_id,
                      double x, double y, double width, double height,
                      uint16_t key_code, uint64_t flags);
void dcc_computer_free_string(char *value);
void dcc_computer_free_bytes(uint8_t *value);
