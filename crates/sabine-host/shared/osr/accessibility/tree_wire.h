#ifndef SABINE_CEF_HOST_OSR_ACCESSIBILITY_TREE_WIRE_H_
#define SABINE_CEF_HOST_OSR_ACCESSIBILITY_TREE_WIRE_H_

#include <vector>

#include "include/cef_values.h"

namespace sabine_osr {

// Chromium's accessibility tree updates of one frame, cut down to what the
// window exposes to assistive technology, in the window's wire format.
std::vector<char> EncodeAccessibilityTree(CefRefPtr<CefDictionaryValue> event);

// The nodes of one frame that moved, in the window's wire format.
std::vector<char> EncodeAccessibilityLocations(CefRefPtr<CefListValue> changes);

}  // namespace sabine_osr

#endif
