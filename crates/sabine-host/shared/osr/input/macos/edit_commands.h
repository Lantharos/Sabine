#ifndef SABINE_CEF_HOST_OSR_INPUT_MACOS_EDIT_COMMANDS_H_
#define SABINE_CEF_HOST_OSR_INPUT_MACOS_EDIT_COMMANDS_H_

#include "include/cef_browser.h"

namespace sabine_osr {

// macOS sends Command shortcuts to the Edit menu rather than to the page, and
// an offscreen browser has no menu, so the ones the page leaves unhandled run
// their edit command on the focused frame here.
bool RunEditCommand(CefRefPtr<CefBrowser> browser, const CefKeyEvent& event);

}  // namespace sabine_osr

#endif
