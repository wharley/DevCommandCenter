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

// Effect results. Mirrored by `NativeStatus` codes in computer_use_commands.rs.
#define DCC_COMPUTER_OK 0
#define DCC_COMPUTER_ERR_PERMISSION 1
#define DCC_COMPUTER_ERR_INVALID 2
#define DCC_COMPUTER_ERR_TARGET_UNAVAILABLE 3
#define DCC_COMPUTER_ERR_FOCUS 4
#define DCC_COMPUTER_ERR_OCCLUDED 5
#define DCC_COMPUTER_ERR_OUTSIDE_TARGET 6
#define DCC_COMPUTER_ERR_ELEMENT_STALE 7
#define DCC_COMPUTER_ERR_ELEMENT_OUTSIDE 8
#define DCC_COMPUTER_ERR_UNSUPPORTED 9
#define DCC_COMPUTER_ERR_EVENT 10

#define DCC_COMPUTER_BUTTON_LEFT 0
#define DCC_COMPUTER_BUTTON_RIGHT 1

#define DCC_COMPUTER_METHOD_ACCESSIBILITY 1
#define DCC_COMPUTER_METHOD_POINTER 2

#define DCC_COMPUTER_WEB_AX_UNSUPPORTED 0
#define DCC_COMPUTER_WEB_AX_ALREADY_ENABLED 1
#define DCC_COMPUTER_WEB_AX_CHANGED 2

// Effects carry the complete target snapshot.  The bridge freshly enumerates
// and compares every field immediately before it captures or posts an event.
// `detail`, when set, is a bridge-allocated string (the occluding app name).
bool dcc_computer_capture_png(const char *bundle_id, int32_t pid, uint32_t window_id,
                              double x, double y, double width, double height,
                              uint8_t **bytes, size_t *length);
// JSON: {status, available, handle, count, text, truncated, busy}. A non-zero
// handle names the observed elements until dcc_computer_ax_tree_release.
char *dcc_computer_ax_tree_json(const char *bundle_id, int32_t pid, uint32_t window_id,
                                double x, double y, double width, double height);
void dcc_computer_ax_tree_release(uint64_t handle);
int32_t dcc_computer_web_accessibility(int32_t pid, bool enabled);
int32_t dcc_computer_click_point(const char *bundle_id, int32_t pid, uint32_t window_id,
                                 double x, double y, double width, double height,
                                 double click_x, double click_y, int32_t button,
                                 uint8_t click_count, char **detail);
int32_t dcc_computer_click_element(const char *bundle_id, int32_t pid, uint32_t window_id,
                                   double x, double y, double width, double height,
                                   uint64_t handle, uint32_t index, int32_t button,
                                   uint8_t click_count, int32_t *method, char **detail);
int32_t dcc_computer_scroll_point(const char *bundle_id, int32_t pid, uint32_t window_id,
                                  double x, double y, double width, double height,
                                  double scroll_x, double scroll_y,
                                  int32_t delta_x, int32_t delta_y, char **detail);
int32_t dcc_computer_scroll_element(const char *bundle_id, int32_t pid, uint32_t window_id,
                                    double x, double y, double width, double height,
                                    uint64_t handle, uint32_t index,
                                    int32_t delta_x, int32_t delta_y, char **detail);
int32_t dcc_computer_drag(const char *bundle_id, int32_t pid, uint32_t window_id,
                          double x, double y, double width, double height,
                          double from_x, double from_y, double to_x, double to_y,
                          char **detail);
int32_t dcc_computer_set_value(const char *bundle_id, int32_t pid, uint32_t window_id,
                               double x, double y, double width, double height,
                               uint64_t handle, uint32_t index, const char *value,
                               char **detail);
int32_t dcc_computer_type(const char *bundle_id, int32_t pid, uint32_t window_id,
                          double x, double y, double width, double height,
                          const char *text);
int32_t dcc_computer_key(const char *bundle_id, int32_t pid, uint32_t window_id,
                         double x, double y, double width, double height,
                         uint16_t key_code, uint64_t flags);
void dcc_computer_free_string(char *value);
void dcc_computer_free_bytes(uint8_t *value);
