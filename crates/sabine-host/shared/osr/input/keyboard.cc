#include "osr/handler.h"

#include "include/wrapper/cef_helpers.h"
#include "osr/input/intercept.h"
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
// unhandled: the app's menu accelerators, zooming the page in, out and back,
// and on macOS the edit menu's.
bool SabineOsrHandler::OnKeyEvent(CefRefPtr<CefBrowser> browser,
                                  const CefKeyEvent& event,
                                  CefEventHandle os_event) {
  CEF_REQUIRE_UI_THREAD();
  if (event.type != KEYEVENT_RAWKEYDOWN) {
    return false;
  }
  if (const KeyChord* chord = MatchKeyChord(menu_accelerators_, event.modifiers,
                                            event.windows_key_code)) {
    transport_.Send(kMenuAccelerator, 0, 0, 0, 0, chord->accelerator.data(),
                    static_cast<uint32_t>(chord->accelerator.size()));
    return true;
  }
  if ((event.modifiers & kChordModifiers) != kShortcutModifier) {
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

// Runs an edit item of the app's menus on the frame that has focus.
void SabineOsrHandler::RunMenuEdit(CefRefPtr<CefBrowser> browser,
                                   const std::string& command) {
  CefRefPtr<CefFrame> frame = browser->GetFocusedFrame();
  if (!frame) {
    return;
  }
  if (command == "undo") {
    frame->Undo();
  } else if (command == "redo") {
    frame->Redo();
  } else if (command == "cut") {
    frame->Cut();
  } else if (command == "copy") {
    frame->Copy();
  } else if (command == "paste") {
    frame->Paste();
  } else if (command == "selectAll") {
    frame->SelectAll();
  }
}
