#include "osr/handler.h"

#include "include/wrapper/cef_helpers.h"

// macOS sends Command shortcuts to the Edit menu rather than to the page, and
// an offscreen browser has no menu, so the ones the page leaves unhandled run
// their edit command on the focused frame here.
bool SabineOsrHandler::OnKeyEvent(CefRefPtr<CefBrowser> browser,
                                  const CefKeyEvent& event,
                                  CefEventHandle os_event) {
  CEF_REQUIRE_UI_THREAD();
  constexpr uint32_t kChordModifiers =
      EVENTFLAG_COMMAND_DOWN | EVENTFLAG_CONTROL_DOWN | EVENTFLAG_ALT_DOWN;
  if (event.type != KEYEVENT_RAWKEYDOWN ||
      (event.modifiers & kChordModifiers) != EVENTFLAG_COMMAND_DOWN) {
    return false;
  }
  CefRefPtr<CefFrame> frame = browser->GetFocusedFrame();
  if (!frame) {
    return false;
  }
  const bool shift = (event.modifiers & EVENTFLAG_SHIFT_DOWN) != 0;
  switch (event.windows_key_code) {
    case 'A':
      frame->SelectAll();
      return true;
    case 'C':
      frame->Copy();
      return true;
    case 'V':
      if (shift) {
        frame->PasteAndMatchStyle();
      } else {
        frame->Paste();
      }
      return true;
    case 'X':
      frame->Cut();
      return true;
    case 'Z':
      if (shift) {
        frame->Redo();
      } else {
        frame->Undo();
      }
      return true;
    default:
      return false;
  }
}
