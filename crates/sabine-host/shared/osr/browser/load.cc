#include "osr/handler.h"

#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/strings.h"
#include "osr/guest/events.h"

using namespace sabine_osr;

void SabineOsrHandler::OnLoadError(CefRefPtr<CefBrowser> browser,
                                   CefRefPtr<CefFrame> frame,
                                   ErrorCode errorCode,
                                   const CefString& errorText,
                                   const CefString& failedUrl) {
  CEF_REQUIRE_UI_THREAD();
  if (errorCode == ERR_ABORTED) {
    return;
  }
  if (GuestView* guest = GuestForBrowser(browser)) {
    std::cerr << "guest " << guest->id
              << " load failed: " << errorText.ToString()
              << " url=" << failedUrl.ToString() << std::endl;
    if (guest->id == kSabinePopupGuestId) {
      DestroyGuest(guest->id);
    }
    return;
  }
  std::stringstream body;
  body << "<!doctype html><meta charset=\"utf-8\"><body style=\"margin:0;"
          "font:14px system-ui;background:#111;color:#eee;padding:24px\">"
       << "<h2>Failed to load</h2><p>" << HtmlEscape(failedUrl.ToString())
       << "</p><p>" << HtmlEscape(errorText.ToString()) << "</p></body>";
  frame->LoadURL(HtmlDataUri(body.str()));
}

void SabineOsrHandler::OnLoadStart(CefRefPtr<CefBrowser> browser,
                                   CefRefPtr<CefFrame> frame,
                                   TransitionType transition_type) {
  CEF_REQUIRE_UI_THREAD();
  if (!frame->IsMain()) {
    return;
  }
  GuestView* guest = GuestForBrowser(browser);
  if (!guest) {
    SendMessage(kMainLoadStarted, 0, 0, 0, 0, nullptr, 0);
    InstallTransparentBackground(frame);
    if (!window_.Floating()) {
      DispatchToplevelState();
    }
  }
}

void SabineOsrHandler::OnLoadEnd(CefRefPtr<CefBrowser> browser,
                                 CefRefPtr<CefFrame> frame,
                                 int httpStatusCode) {
  CEF_REQUIRE_UI_THREAD();
  if (!frame->IsMain()) {
    return;
  }
  GuestView* guest = GuestForBrowser(browser);
  if (!guest) {
    InstallTransparentBackground(frame);
    SendMessage(kMainLoadReady, 0, 0, 0, 0, nullptr, 0);
    if (!window_.visible || window_.suspended) {
      DispatchWindowState();
    }
    if (!window_.Floating()) {
      DispatchToplevelState();
    }
    WatchAcceleratedPaint();
    return;
  }
  guest->url = frame->GetURL();
  EmitGuestEvent(*guest, "guest.navigated", GuestNavigatedJson(*guest));
}

void SabineOsrHandler::OnLoadingStateChange(CefRefPtr<CefBrowser> browser,
                                            bool isLoading,
                                            bool canGoBack,
                                            bool canGoForward) {
  CEF_REQUIRE_UI_THREAD();
  GuestView* guest = GuestForBrowser(browser);
  if (!guest || guest->loading == isLoading) {
    return;
  }
  guest->loading = isLoading;
  EmitGuestEvent(*guest, "guest.loading",
                 "{\"id\":" + JsString(guest->id) +
                     ",\"loading\":" + (isLoading ? "true" : "false") + "}");
}

void SabineOsrHandler::OnTitleChange(CefRefPtr<CefBrowser> browser,
                                     const CefString& title) {
  CEF_REQUIRE_UI_THREAD();
  GuestView* guest = GuestForBrowser(browser);
  if (!guest) {
    return;
  }
  guest->title = title.ToString();
  EmitGuestEvent(*guest, "guest.title",
                 "{\"id\":" + JsString(guest->id) +
                     ",\"title\":" + JsString(guest->title) + "}");
}

void SabineOsrHandler::OnAddressChange(CefRefPtr<CefBrowser> browser,
                                       CefRefPtr<CefFrame> frame,
                                       const CefString& url) {
  CEF_REQUIRE_UI_THREAD();
  if (!frame->IsMain()) {
    return;
  }
  GuestView* guest = GuestForBrowser(browser);
  if (!guest) {
    return;
  }
  guest->url = url.ToString();
  EmitGuestEvent(*guest, "guest.navigated", GuestNavigatedJson(*guest));
}

void SabineOsrHandler::OnFaviconURLChange(
    CefRefPtr<CefBrowser> browser,
    const std::vector<CefString>& icon_urls) {
  CEF_REQUIRE_UI_THREAD();
  GuestView* guest = GuestForBrowser(browser);
  if (!guest) {
    return;
  }
  std::vector<std::string> favicons;
  favicons.reserve(icon_urls.size());
  for (const CefString& icon : icon_urls) {
    favicons.push_back(icon.ToString());
  }
  EmitGuestEvent(*guest, "guest.favicon",
                 GuestFaviconJson(guest->id, favicons));
}
