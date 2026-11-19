#include "osr/handler.h"

#include <cstdio>

#include "common/trace.h"
#include "include/cef_app.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/registry.h"
#include "sabine_host_protocol.h"
#if defined(OS_LINUX)
#include "linux/runtime/server.h"
#endif

using namespace sabine_osr;

void SabineOsrHandler::OnAfterCreated(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  browsers_.push_back(browser);
  if (closing_ || close_requested_) {
    browser->GetHost()->CloseBrowser(true);
    return;
  }
  const bool primary_browser = !browser_;
  if (primary_browser) {
    browser_ = browser;
  } else if (GuestView* pending = guests_.AwaitingBrowser()) {
    pending->browser = browser;
    pending->pending = false;
  }
  CefRefPtr<CefBrowserHost> host = browser->GetHost();
  host->SetWindowlessFrameRate(FrameRate());
  if (!primary_browser) {
    return;
  }
  if (TraceEnabled()) {
    std::fprintf(stderr, "Sabine CEF: primary browser ready windowless=%d\n",
                 host->IsWindowRenderingDisabled() ? 1 : 0);
    std::fflush(stderr);
  }
  if (!transport_.Connect() ||
      !transport_.Send(kHostHello, 0, 0, 0, 0, SABINE_HOST_PROTOCOL_VERSION,
                       sizeof(SABINE_HOST_PROTOCOL_VERSION) - 1)) {
    std::fprintf(stderr, "Sabine OSR: failed to connect native host\n");
    std::fflush(stderr);
    host->CloseBrowser(true);
    return;
  }
  transport_.StartReading();
  SendFocusedImeState();
  media_.Report();
  host->WasResized();
  host->Invalidate(PET_VIEW);
}

bool SabineOsrHandler::OnBeforePopup(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    int popup_id,
    const CefString& target_url,
    const CefString& target_frame_name,
    cef_window_open_disposition_t target_disposition,
    bool user_gesture,
    const CefPopupFeatures& popup_features,
    CefWindowInfo& window_info,
    CefRefPtr<CefClient>& client,
    CefBrowserSettings& settings,
    CefRefPtr<CefDictionaryValue>& extra_info,
    bool* no_javascript_access) {
  guests_.OpenPopup(browser, target_url.ToString(),
                    static_cast<int>(target_disposition), user_gesture);
  return true;
}

bool SabineOsrHandler::DoClose(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  if (browser_ && browser_->IsSame(browser))
    closing_ = true;
  return false;
}

void SabineOsrHandler::OnBeforeClose(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  const int browser_id = browser->GetIdentifier();
  accelerated_.RetireBrowser(browser_id);
  renderer_crashes_.erase(browser_id);
  ime_.Forget(browser_id);
  media_.Forget(browser_id);
  media_.Report();
  clipboard_.Forget(browser);
  permissions_.Forget(browser_id);
  dialogs_->Forget(browser);
  if (devtools_inspected_ && devtools_inspected_->IsSame(browser)) {
    CloseDevTools();
  }
  if (GuestView* guest = guests_.FindByBrowser(browser)) {
    const std::string id = guest->id;
    guest->browser = nullptr;
    guests_.Destroy(id);
  } else if (browser_ && browser_->IsSame(browser)) {
    guests_.DestroyAll();
    ClosePrintDocuments();
    accessibility_->Forget();
    browser_ = nullptr;
  }
  browsers_.remove_if([&](const CefRefPtr<CefBrowser>& entry) {
    return entry->IsSame(browser);
  });
  QuitWhenClosed();
}

void SabineOsrHandler::QuitWhenClosed() {
  if (!browsers_.empty() || !print_documents_.empty()) {
    return;
  }
  transport_.Close();
  UnregisterHandler(this);
  if (HasRegisteredHandlers()) {
    return;
  }
#if defined(OS_LINUX)
  if (sabine_runtime::IsSharedRuntime(CefCommandLine::GetGlobalCommandLine())) {
    sabine_runtime::WindowsClosed();
    return;
  }
#endif
  CefQuitMessageLoop();
}
