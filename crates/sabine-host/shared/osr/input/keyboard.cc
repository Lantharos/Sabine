#include "osr/handler.h"

#include "include/wrapper/cef_helpers.h"
#include "osr/input/key_codes.h"
#if defined(OS_MAC)
#include "osr/input/macos/edit_commands.h"
#endif

using namespace sabine_osr;

namespace {

#if defined(OS_MAC)
constexpr uint32_t kShortcutModifier = EVENTFLAG_COMMAND_DOWN;
#else
constexpr uint32_t kShortcutModifier = EVENTFLAG_CONTROL_DOWN;
#endif
constexpr uint32_t kChordModifiers =
    EVENTFLAG_COMMAND_DOWN | EVENTFLAG_CONTROL_DOWN | EVENTFLAG_ALT_DOWN;

bool ZoomCommandForKey(int key_code, cef_zoom_command_t* command) {
  switch (key_code) {
    case kKeyCodeEquals:
      *command = CEF_ZOOM_COMMAND_IN;
      return true;
    case kKeyCodeMinus:
      *command = CEF_ZOOM_COMMAND_OUT;
      return true;
    case '0':
      *command = CEF_ZOOM_COMMAND_RESET;
      return true;
    default:
      return false;
  }
}

}  // namespace

// Runs the shortcuts a browser window would for keys the page leaves
// unhandled: zooming the page in, out and back, and on macOS the edit menu's.
bool SabineOsrHandler::OnKeyEvent(CefRefPtr<CefBrowser> browser,
                                  const CefKeyEvent& event,
                                  CefEventHandle os_event) {
  CEF_REQUIRE_UI_THREAD();
  if (event.type != KEYEVENT_RAWKEYDOWN ||
      (event.modifiers & kChordModifiers) != kShortcutModifier) {
    return false;
  }
  cef_zoom_command_t zoom;
  if (browser_ && browser_->IsSame(browser) &&
      ZoomCommandForKey(event.windows_key_code, &zoom)) {
    browser->GetHost()->Zoom(zoom);
    ReportPageZoom();
    return true;
  }
#if defined(OS_MAC)
  return RunEditCommand(browser, event);
#else
  return false;
#endif
}
