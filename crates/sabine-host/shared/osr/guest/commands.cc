#include "osr/handler.h"

#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include "common/bridge_policy.h"
#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/guest/events.h"

using namespace sabine_osr;

bool SabineOsrHandler::RunGuestOperation(const std::string& operation,
                                         const JsonObject& payload,
                                         std::string* response,
                                         std::string* error) {
  CEF_REQUIRE_UI_THREAD();
  *response = "{}";
  if (operation == "list") {
    const GuestRegistry& registry = guests_;
    std::vector<const GuestView*> listed;
    for (const GuestView* guest : registry.InZOrder()) {
      if (guest->ShownToPage()) {
        listed.push_back(guest);
      }
    }
    *response = GuestListJson(listed);
    return true;
  }
  if (operation == "setCovered") {
    const bool covered = payload.Bool("covered", false);
    if (guests_.Covered() == covered) {
      return true;
    }
    guests_.SetCovered(covered);
    pending_guest_cover_ = covered;
    if (covered) {
      FocusGuest(std::string());
    }
    for (GuestView* guest : guests_.InZOrder()) {
      ApplyGuestVisibility(*guest);
    }
    if (covered && browser_) {
      browser_->GetHost()->Invalidate(PET_VIEW);
    }
    return true;
  }
  if (operation == "downloadAction") {
    return downloads_->RunGuestAction(payload, error);
  }

  const std::string id = payload.String("id");
  if (id.empty()) {
    *error = "guest." + operation + " requires an `id`";
    return false;
  }
  GuestView* guest = guests_.Find(id);
  if (guest && !guest->ShownToPage()) {
    *error = "unknown guest id";
    return false;
  }
  if (operation == "destroy") {
    if (!guest && !HasPendingGuest(id)) {
      *error = "unknown guest id";
      return false;
    }
    DestroyGuest(id);
    *response = GuestIdJson(id);
    return true;
  }

  if (!guest) {
    *error = "unknown guest id";
    return false;
  }
  CefRefPtr<CefBrowser> browser = guest->browser;
  if (operation == "get") {
    *response = GuestInfoJson(*guest);
    return true;
  }
  if (operation == "setBounds") {
    guest->bounds = ParseGuestBounds(payload, guest->bounds);
    ApplyGuestBounds(*guest);
    *response = GuestInfoJson(*guest);
    return true;
  }
  if (operation == "setVisible") {
    const bool visible = payload.Bool("visible", true);
    if (guest->visible != visible) {
      guest->visible = visible;
      ApplyGuestVisibility(*guest);
    }
    *response = GuestInfoJson(*guest);
    return true;
  }
  if (operation == "focus") {
    FocusGuest(id);
    return true;
  }
  if (!browser) {
    *error = "guest browser is not ready";
    return false;
  }
  if (operation == "navigate") {
    const std::string url = payload.String("url");
    const std::string html = payload.String("html");
    if (url.empty() && html.empty()) {
      *error = "guest.navigate requires a `url` or `html`";
      return false;
    }
    guest->url = url.empty()
                     ? sabine_bridge::TrustedHtmlUrl(guest->bridge_policy, html)
                     : url;
    browser->GetMainFrame()->LoadURL(guest->url);
    return true;
  }
  if (operation == "reload") {
    if (payload.Bool("ignoreCache", false)) {
      browser->ReloadIgnoreCache();
    } else {
      browser->Reload();
    }
    return true;
  }
  if (operation == "goBack") {
    if (!browser->CanGoBack()) {
      *error = "guest cannot go back";
      return false;
    }
    browser->GoBack();
    return true;
  }
  if (operation == "goForward") {
    if (!browser->CanGoForward()) {
      *error = "guest cannot go forward";
      return false;
    }
    browser->GoForward();
    return true;
  }
  if (operation == "setZoom") {
    guest->zoom = ClampGuestZoom(payload.Number("factor", 1.0));
    browser->GetHost()->SetZoomLevel(GuestZoomLevel(guest->zoom));
    *response = GuestInfoJson(*guest);
    return true;
  }
  if (operation == "executeJavaScript") {
    const std::string code = payload.String("code");
    if (code.empty()) {
      *error = "guest.executeJavaScript requires `code`";
      return false;
    }
    CefRefPtr<CefFrame> frame = browser->GetMainFrame();
    frame->ExecuteJavaScript(code, frame->GetURL(), 0);
    return true;
  }
  *error = "unsupported guest operation";
  return false;
}

bool SabineOsrHandler::HandleGuestBridgeCommand(const std::string& command,
                                                const std::string& payload,
                                                const std::string& browser_id,
                                                const std::string& request_id) {
  const JsonObject fields = JsonObject::Parse(payload);
  if (command == "sabine.popup.open") {
    GuestCreateRequest request;
    std::string parse_error;
    if (!ParseGuestCreateRequest(fields, &request, &parse_error)) {
      ResolveBridgeResponse(browser_id, request_id, false,
                            JsonMessage(parse_error));
      return true;
    }
    request.id = kSabinePopupGuestId;
    request.allow_bridge = false;
    request.allow_downloads = false;
    request.visible = true;
    CefRefPtr<SabineOsrHandler> self(this);
    CreateGuest(std::move(request),
                [self, browser_id, request_id](bool success,
                                               const std::string& result) {
                  self->ResolveBridgeResponse(
                      browser_id, request_id, success,
                      success ? "{\"accepted\":true}" : JsonMessage(result));
                });
    return true;
  }
  if (command == "sabine.popup.close") {
    DismissPopupGuest();
    ResolveBridgeResponse(browser_id, request_id, true, "{}");
    return true;
  }
  if (!IsGuestBridgeCommand(command)) {
    return false;
  }

  const std::string operation = GuestOperationName(command, kGuestBridgePrefix);
  if (operation == "create") {
    GuestCreateRequest request;
    std::string error;
    if (!ParseGuestCreateRequest(fields, &request, &error)) {
      ResolveBridgeResponse(browser_id, request_id, false, JsonMessage(error));
      return true;
    }
    if (request.id.empty()) {
      request.id = NextGuestId();
    }
    CefRefPtr<SabineOsrHandler> self(this);
    CreateGuest(
        std::move(request), [self, browser_id, request_id](
                                bool success, const std::string& result) {
          self->ResolveBridgeResponse(browser_id, request_id, success,
                                      success ? result : JsonMessage(result));
        });
    return true;
  }
  if (operation == "capturePreview") {
    const std::string id = fields.String("id");
    GuestView* guest = guests_.Find(id);
    if (!guest || !guest->browser || !guest->ShownToPage()) {
      ResolveBridgeResponse(browser_id, request_id, false,
                            JsonMessage("unknown guest id"));
      return true;
    }
    if (!guest->visible || view_hidden_ || guests_.Covered()) {
      ResolveBridgeResponse(browser_id, request_id, false,
                            JsonMessage("guest browser is not visible"));
      return true;
    }
    std::string request = browser_id;
    request.push_back('\0');
    request += request_id;
    request.push_back('\0');
    request += id;
    if (!SendMessage(kGuestCaptureRequested, 0, 0, 0, 0, request.data(),
                     static_cast<uint32_t>(request.size()))) {
      ResolveBridgeResponse(browser_id, request_id, false,
                            JsonMessage("failed to request guest capture"));
    }
    return true;
  }
  std::string response;
  std::string error;
  if (!RunGuestOperation(operation, fields, &response, &error)) {
    ResolveBridgeResponse(browser_id, request_id, false, JsonMessage(error));
    return true;
  }
  ResolveBridgeResponse(browser_id, request_id, true, response);
  return true;
}

void SabineOsrHandler::ApplyHostControl(const std::string& command,
                                        const std::string& value) {
  CEF_REQUIRE_UI_THREAD();
  const std::string operation =
      GuestOperationName(command, kGuestHostControlPrefix);
  if (operation.empty()) {
    return;
  }
  const JsonObject fields = JsonObject::Parse(value);
  if (operation == "create") {
    GuestCreateRequest request;
    std::string error;
    if (!ParseGuestCreateRequest(fields, &request, &error)) {
      std::cerr << "guest.create failed: " << error << std::endl;
      return;
    }
    if (request.id.empty()) {
      request.id = NextGuestId();
    }
    CreateGuest(std::move(request),
                [](bool success, const std::string& result) {
                  if (!success) {
                    std::cerr << "guest.create failed: " << result << std::endl;
                  }
                });
    return;
  }
  std::string response;
  std::string error;
  if (!RunGuestOperation(operation, fields, &response, &error)) {
    std::cerr << "guest." << operation << " failed: " << error << std::endl;
  }
}
