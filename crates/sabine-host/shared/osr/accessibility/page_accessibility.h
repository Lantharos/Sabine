#ifndef SABINE_CEF_HOST_OSR_ACCESSIBILITY_PAGE_ACCESSIBILITY_H_
#define SABINE_CEF_HOST_OSR_ACCESSIBILITY_PAGE_ACCESSIBILITY_H_

#include <string>
#include <vector>

#include "include/cef_accessibility_handler.h"
#include "include/cef_browser.h"
#include "osr/accessibility/actions.h"
#include "osr/transport/transport.h"

// The page's accessibility tree, which Chromium builds only while assistive
// technology reads the window, and which the window hands to the desktop.
class PageAccessibility : public CefAccessibilityHandler {
 public:
  explicit PageAccessibility(sabine_osr::Transport& transport)
      : transport_(transport), actions_(new AccessibilityActions) {}

  void Enable(CefRefPtr<CefBrowser> browser, bool enabled);
  void Perform(CefRefPtr<CefBrowser> browser,
               const std::vector<std::string>& parts) {
    actions_->Perform(browser, parts);
  }
  void Forget() { actions_->Forget(); }

  void OnAccessibilityTreeChange(CefRefPtr<CefValue> value) override;
  void OnAccessibilityLocationChange(CefRefPtr<CefValue> value) override;

 private:
  sabine_osr::Transport& transport_;
  CefRefPtr<AccessibilityActions> actions_;
  bool enabled_ = false;

  IMPLEMENT_REFCOUNTING(PageAccessibility);
};

#endif
