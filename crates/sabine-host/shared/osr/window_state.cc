#include "osr/window_state.h"

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
    host->SetWindowlessFrameRate(frame_rate);
    if (!UpdateViewHidden() && needs_paint) {
      host->Invalidate(PET_VIEW);
    }
    ApplyGuestLifecycle();
    return;
  }
  suspended_ = true;
  resume_needs_paint_ = resume_needs_paint_ || reason == "hidden";
  host->SetWindowlessFrameRate(frame_rate);
  ApplyGuestLifecycle();
}

void SabineOsrHandler::RepaintAll() {
  if (!browser_) {
    return;
  }
  browser_->GetHost()->Invalidate(PET_VIEW);
  browser_->GetHost()->Invalidate(PET_POPUP);
  for (auto* guest : guests_.InZOrder()) {
    if (guest->browser && guest->visible) {
      guest->browser->GetHost()->Invalidate(PET_VIEW);
      guest->browser->GetHost()->Invalidate(PET_POPUP);
    }
  }
}

void SabineOsrHandler::ApplyWindowState(bool shown,
                                        bool occluded,
                                        bool suspended) {
  CEF_REQUIRE_UI_THREAD();
  window_.shown = shown;
  window_.visible = shown && !occluded;
  window_.occluded = shown && occluded;
  window_.suspended = suspended;
  if (browser_ && UpdateViewHidden()) {
    ApplyGuestLifecycle();
  }
  DispatchWindowState();
}

bool SabineOsrHandler::UpdateViewHidden() {
  const bool hidden =
      window_.occluded || (!window_.shown && !retain_hidden_frame_);
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
      (window_.visible ? "true" : "false") + "," +
      (window_.suspended ? "true" : "false") + ");";
  for (auto& browser : browsers_) {
    if (!GuestForBrowser(browser)) {
      browser->GetMainFrame()->ExecuteJavaScript(
          script, browser->GetMainFrame()->GetURL(), 0);
    }
  }
}

void SabineOsrHandler::ApplyToplevelState(bool maximized,
                                          bool fullscreen,
                                          bool tiled) {
  CEF_REQUIRE_UI_THREAD();
  window_.maximized = maximized;
  window_.fullscreen = fullscreen;
  window_.tiled = tiled;
  DispatchToplevelState();
}

void SabineOsrHandler::DispatchToplevelState() {
  CEF_REQUIRE_UI_THREAD();
  const auto flag = [](bool value) { return value ? "true" : "false"; };
  const std::string script =
      std::string("window.__sabineToplevelSet&&window.__sabineToplevelSet(") +
      flag(window_.maximized) + "," + flag(window_.fullscreen) + "," +
      flag(window_.tiled) + ");";
  for (auto& browser : browsers_) {
    if (!GuestForBrowser(browser)) {
      browser->GetMainFrame()->ExecuteJavaScript(
          script, browser->GetMainFrame()->GetURL(), 0);
    }
  }
}
