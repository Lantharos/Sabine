#ifndef SABINE_CEF_HOST_OSR_INPUT_DRAG_H_
#define SABINE_CEF_HOST_OSR_INPUT_DRAG_H_

#include <cstdint>
#include <string>
#include <vector>

#include "include/cef_browser.h"
#include "include/cef_drag_data.h"

// A drag the page started, and one the window carries over the page.
struct DragSession {
  CefRefPtr<CefBrowser> source_browser;
  CefRefPtr<CefDragData> source_data;
  cef_drag_operations_mask_t source_operations = DRAG_OPERATION_NONE;
  cef_drag_operations_mask_t operation = DRAG_OPERATION_NONE;
  cef_drag_operations_mask_t target_operations = DRAG_OPERATION_NONE;
  CefMouseEvent target_event;
  uint64_t over_serial = 0;
  uint64_t dropped_paths_serial = 0;
  bool awaiting_dropped_paths = false;
  std::vector<std::vector<std::string>> held_controls;
};

#endif
