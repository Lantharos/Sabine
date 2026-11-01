#ifndef SABINE_CEF_HOST_OSR_INPUT_IME_H_
#define SABINE_CEF_HOST_OSR_INPUT_IME_H_

#include <map>
#include <string>
#include <vector>

#include "include/cef_browser.h"

namespace sabine_osr {

// Returns true when |parts| is an IME control line and was handled.
bool TryHandleImeControl(CefRefPtr<CefBrowserHost> host,
                         const std::vector<std::string>& parts);

}  // namespace sabine_osr

// What each browser's focused editable tells the input method: its input
// mode, caret, and surrounding text, with the frame that reported the text.
struct ImeState {
  std::map<int, cef_text_input_mode_t> modes;
  std::map<int, CefRect> cursor_rects;
  std::map<int, std::string> surrounding;
  std::map<int, CefRefPtr<CefFrame>> frames;

  void Forget(int browser_id);
};

#endif
