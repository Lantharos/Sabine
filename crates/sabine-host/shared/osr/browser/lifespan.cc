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
  } else if (GuestView* pending = GuestAwaitingBrowser()) {
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
  if (!socket_.Connect(endpoint_, authentication_token_) ||
      !SendMessage(kHostHello, 0, 0, 0, 0, SABINE_HOST_PROTOCOL_VERSION,
                   sizeof(SABINE_HOST_PROTOCOL_VERSION) - 1)) {
    std::fprintf(stderr, "Sabine OSR: failed to connect native host\n");
    std::fflush(stderr);
    host->CloseBrowser(true);
    return;
  }
  StartCommandReader();
  SendFocusedImeState();
  ReportMediaPlaying();
  host->WasResized();
  host->Invalidate(PET_VIEW);
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
  RetireAcceleratedBrowser(browser_id);
  ForgetAcceleratedCaptures(browser_id);
  renderer_crashes_.erase(browser_id);
  ime_.Forget(browser_id);
  media_.Forget(browser_id);
  ReportMediaPlaying();
  clipboard_.Forget(browser);
  if (devtools_inspected_ && devtools_inspected_->IsSame(browser)) {
    CloseDevTools();
  }
  if (GuestView* guest = guests_.FindByBrowser(browser)) {
    const std::string id = guest->id;
    guest->browser = nullptr;
    DestroyGuest(id);
  } else if (browser_ && browser_->IsSame(browser)) {
    std::vector<std::string> ids;
    for (const GuestView* view : guests_.InZOrder()) {
      ids.push_back(view->id);
    }
    for (const auto& id : ids) {
      DestroyGuest(id);
    }
    ClosePrintDocuments();
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
  CloseTransport();
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
