#include "osr/input/macos/edit_commands.h"

namespace sabine_osr {

bool RunEditCommand(CefRefPtr<CefBrowser> browser, const CefKeyEvent& event) {
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

}  // namespace sabine_osr
