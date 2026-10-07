#include <algorithm>
#include <string>

#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

using namespace sabine_osr;

void SabineOsrHandler::ApplyLifecycle(const std::string& state,
                                      int frame_rate,
                                      const std::string& reason) {
  CEF_REQUIRE_UI_THREAD();
  if (!browser_) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = browser_->GetHost();
  SetPagesFrozen(state == "frozen");
  if (state == "active") {
    const bool needs_paint = resume_needs_paint_;
    suspended_ = false;
    resume_needs_paint_ = false;
    host->SetWindowlessFrameRate(std::max(1, frame_rate));
    if (!UpdateViewHidden() && needs_paint) {
      host->Invalidate(PET_VIEW);
    }
    ApplyGuestLifecycle();
    return;
  }
  suspended_ = true;
  resume_needs_paint_ = resume_needs_paint_ || reason == "hidden";
  host->SetWindowlessFrameRate(std::max(1, frame_rate));
  ApplyGuestLifecycle();
}

void SabineOsrHandler::ApplyWindowState(bool shown,
                                        bool occluded,
                                        bool suspended) {
  CEF_REQUIRE_UI_THREAD();
  window_shown_ = shown;
  window_visible_ = shown && !occluded;
  window_occluded_ = shown && occluded;
  window_suspended_ = suspended;
  if (browser_ && UpdateViewHidden()) {
    ApplyGuestLifecycle();
  }
  DispatchWindowState();
}

bool SabineOsrHandler::UpdateViewHidden() {
  const bool hidden =
      window_occluded_ || (!window_shown_ && !retain_hidden_frame_);
  if (!browser_ || hidden == view_hidden_) {
    return false;
  }
  view_hidden_ = hidden;
  CefRefPtr<CefBrowserHost> host = browser_->GetHost();
  host->WasHidden(hidden);
  if (!hidden) {
    host->WasResized();
    host->Invalidate(PET_VIEW);
  }
  return true;
}

void SabineOsrHandler::DispatchWindowState() {
  CEF_REQUIRE_UI_THREAD();
  const std::string script =
      std::string(
          "window.__sabineWindowStateSet&&window.__sabineWindowStateSet(") +
      (window_visible_ ? "true" : "false") + "," +
      (window_suspended_ ? "true" : "false") + ");";
  for (auto& browser : browsers_) {
    if (!GuestForBrowser(browser)) {
      browser->GetMainFrame()->ExecuteJavaScript(
          script, browser->GetMainFrame()->GetURL(), 0);
    }
  }
}
