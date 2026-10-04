#include "osr/handler.h"

#include <cstdint>
#include <cstdlib>
#include <string>
#include <vector>

#include "guest/input.h"
#include "guest/manager.h"
#include "include/cef_app.h"
#include "include/cef_browser.h"
#include "include/cef_request_context_handler.h"
#include "include/cef_task.h"
#include "include/internal/cef_types.h"
#include "include/wrapper/cef_helpers.h"
#include "common/bytes_message.h"
#include "common/json.h"
#include "common/bridge_policy.h"
#include "osr/utilities.h"

using namespace sabine_osr;

bool SabineOsrHandler::OnProcessMessageReceived(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefProcessId source_process,
    CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_UI_THREAD();
  if (source_process != PID_RENDERER || !message || !browser || !frame ||
      !frame->IsValid() || !BridgePolicyFor(browser)) {
    return false;
  }
  if (message->GetName() == "sabine.native.bytes") {
    sabine_bytes::Message bytes;
    if (sabine_bytes::Read(message, &bytes))
      ReceiveNativeMessage(browser, frame, bytes.fields,
                           std::string(bytes.body, bytes.body_size));
    return true;
  }
  CefRefPtr<CefListValue> arguments = message->GetArgumentList();
  if (!arguments || arguments->GetSize() < 1) {
    return true;
  }
  for (size_t index = 0; index < arguments->GetSize(); ++index) {
    if (arguments->GetType(index) != VTYPE_STRING) {
      return true;
    }
  }
  if (message->GetName() == "sabine.clipboard") {
    if (arguments->GetSize() == 1)
      ForwardClipboardRequest(browser, frame, arguments->GetString(0));
    return true;
  }
  if (message->GetName() == "sabine.ime_state") {
    const int browser_id = browser->GetIdentifier();
    ime_surrounding_state_[browser_id] = arguments->GetString(0);
    ime_frames_[browser_id] = frame;
    SendFocusedImeState();
    return true;
  }
  if (message->GetName() != "sabine.native") {
    return false;
  }
  std::vector<std::string> values;
  for (size_t index = 0; index < arguments->GetSize(); ++index)
    values.push_back(arguments->GetString(index));
  ReceiveNativeMessage(browser, frame, values, std::nullopt);
  return true;
}

void SabineOsrHandler::ReceiveNativeMessage(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    const std::vector<std::string>& values,
    const std::optional<std::string>& body) {
  if (!frame->IsMain() || values.size() < 3)
    return;
  const auto policy = BridgePolicyFor(browser);
  const std::string url = frame->GetURL();
  if (!sabine_bridge::MatchesSecurityOrigin(policy, url, values[0]))
    return;
  const std::string& kind = values[1];
  if (kind == "window") {
    if (values.size() == 4 && sabine_bridge::AllowsDocument(policy, url))
      HandleWindowCommand(values[2], values[3]);
    return;
  }
  if (kind != "bridge" || values.size() != 5)
    return;
  const std::string& command = values[3];
  if (!sabine_bridge::AllowsCommand(policy, url, command))
    return;
  HandleBridgeCommand(browser, frame, values[2], command, values[4], body);
}

void SabineOsrHandler::HandleWindowCommand(const std::string& command,
                                           const std::string& value) {
  if (command == "close") {
    // Ask the native host to tear down its window; it replies with "close\n"
    // which CloseBrowsers this surface. Do not quit the shared CEF process.
    RequestNativeClose();
  } else if (command == "start-drag" || command == "drag") {
    SendMessage(kStartDragRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "minimize") {
    SendMessage(kMinimizeRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "maximize") {
    SendMessage(kMaximizeRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "restore") {
    SendMessage(kRestoreRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "toggle-maximize") {
    SendMessage(kToggleMaximizeRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "fullscreen") {
    SendMessage(kFullscreenRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "exit-fullscreen") {
    SendMessage(kExitFullscreenRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "show") {
    SendMessage(kShowRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "hide") {
    SendMessage(kHideRequested, 0, 0, 0, 0, nullptr, 0);
  } else if (command == "focus") {
    SendMessage(kFocusRequested, 0, 0, 0, 0, value.data(),
                static_cast<uint32_t>(value.size()));
  }
}

void SabineOsrHandler::HandleBridgeCommand(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    const std::string& request_id,
    const std::string& command,
    const std::string& payload,
    const std::optional<std::string>& body) {
  const std::string browser_id = std::to_string(browser->GetIdentifier());
  std::string origin = sabine_bridge::Origin(frame->GetURL());
  if (origin.empty())
    origin = frame->GetURL();
  else if (origin.back() == '/')
    origin.pop_back();
  if (request_id.empty() || command.empty()) {
    ResolveBridgeResponse(browser_id, request_id, false,
                          "{\"message\":\"Malformed Sabine bridge request\"}");
    return;
  }
  const GuestView* guest = GuestForBrowser(browser);
  if (guest && !guest->allow_bridge) {
    ResolveBridgeResponse(
        browser_id, request_id, false,
        "{\"message\":\"Sabine bridge is unavailable inside guest views\"}");
    return;
  }
  if (guest) {
    if (IsGuestBridgeCommand(command) || command == "sabine.popup.open" ||
        command == "sabine.popup.close") {
      ResolveBridgeResponse(browser_id, request_id, false,
                            "{\"message\":\"Guest views cannot manage other "
                            "guest views\"}");
      return;
    }
  } else if (HandleGuestBridgeCommand(command, payload, browser_id,
                                      request_id)) {
    return;
  }
  if (bridge_commands_.find(command) == bridge_commands_.end() ||
      command.rfind(kClipboardCommandPrefix, 0) == 0) {
    ResolveBridgeResponse(
        browser_id, request_id, false,
        "{\"message\":\"Sabine bridge command is not allowlisted\"}");
    return;
  }
  std::string request = "SABINE_BRIDGE_REQUEST\t" + browser_id + "\t" +
                        request_id + "\t" + origin + "\t" + command + "\t" +
                        (payload.empty() ? "{}" : payload);
  if (body) {
    if (!bridge_policy_->GetBool("bytes")) {
      ResolveBridgeResponse(
          browser_id, request_id, false,
          "{\"message\":\"This app was built with a Sabine that cannot take "
          "bytes from pages\"}");
      return;
    }
    request = sabine_bytes::kPrefix + std::to_string(body->size()) + "\t" +
              request + "\n" + *body;
  }
  SendMessage(kBridgeRequest, 0, 0, 0, 0, request.data(),
              static_cast<uint32_t>(request.size()));
}

void SabineOsrHandler::RequestNativeClose() {
  SendMessage(kCloseRequested, 0, 0, 0, 0, nullptr, 0);
}

void SabineOsrHandler::CloseFromNativeDisconnect() {
  CEF_REQUIRE_UI_THREAD();
  close_requested_ = true;
  if (browser_) {
    browser_->GetHost()->CloseBrowser(true);
  }
}

CefRefPtr<CefDictionaryValue> SabineOsrHandler::BridgePolicyFor(
    CefRefPtr<CefBrowser> browser) {
  if (!browser)
    return nullptr;
  if (browser_ && browser_->IsSame(browser))
    return bridge_policy_;
  const GuestView* guest = GuestForBrowser(browser);
  return guest ? guest->bridge_policy : nullptr;
}

void SabineOsrHandler::InstallTransparentBackground(CefRefPtr<CefFrame> frame) {
  if (!transparent_background_) {
    return;
  }
  frame->ExecuteJavaScript(
      "(function(){"
      "if(document.documentElement){document.documentElement.style.background='"
      "transparent';}"
      "if(document.body){document.body.style.background='transparent';}"
      "if(!document.querySelector('style[data-sabine-transparent-background]'))"
      "{"
      "const style=document.createElement('style');"
      "style.setAttribute('data-sabine-transparent-background','');"
      "style.textContent='html,body{background:transparent!important;}';"
      "document.head&&document.head.appendChild(style);"
      "}"
      "})();",
      frame->GetURL(), 0);
}

void SabineOsrHandler::ResolveBridgeResponse(
    const std::string& browser_id,
    const std::string& request_id,
    bool ok,
    const std::string& payload,
    const std::optional<std::string>& body) {
  CEF_REQUIRE_UI_THREAD();
  if (ResolveClipboardResponse(request_id, ok, payload))
    return;
  const int expected_id = std::atoi(browser_id.c_str());
  CefRefPtr<CefBrowser> target;
  for (auto& browser : browsers_) {
    if (browser->GetIdentifier() == expected_id) {
      target = browser;
      break;
    }
  }
  if (!target || request_id.empty()) {
    return;
  }
  if (body) {
    if (auto response = sabine_bytes::Create(
            "sabine.response.bytes", {request_id}, body->data(), body->size()))
      target->GetMainFrame()->SendProcessMessage(PID_RENDERER, response);
    return;
  }
  auto response = CefProcessMessage::Create("sabine.response");
  auto values = response->GetArgumentList();
  values->SetString(0, request_id);
  values->SetBool(1, ok);
  values->SetString(2, payload.empty() ? "null" : payload);
  target->GetMainFrame()->SendProcessMessage(PID_RENDERER, response);
}

void SabineOsrHandler::EmitBridgeEvent(const std::string& name_json,
                                       const std::string& payload,
                                       const std::optional<std::string>& body) {
  CEF_REQUIRE_UI_THREAD();
  for (auto& browser : browsers_) {
    if (!sabine_bridge::AllowsDocument(BridgePolicyFor(browser),
                                       browser->GetMainFrame()->GetURL())) {
      continue;
    }
    if (body) {
      if (auto event = sabine_bytes::Create("sabine.event.bytes", {name_json},
                                            body->data(), body->size()))
        browser->GetMainFrame()->SendProcessMessage(PID_RENDERER, event);
      continue;
    }
    auto event = CefProcessMessage::Create("sabine.event");
    event->GetArgumentList()->SetString(0, name_json);
    event->GetArgumentList()->SetString(1, payload.empty() ? "null" : payload);
    browser->GetMainFrame()->SendProcessMessage(PID_RENDERER, event);
  }
}

void SabineOsrHandler::EmitPrimaryEvent(const std::string& name,
                                        const std::string& payload) {
  CEF_REQUIRE_UI_THREAD();
  if (!browser_) {
    return;
  }
  CefRefPtr<CefFrame> frame = browser_->GetMainFrame();
  if (!frame || !sabine_bridge::AllowsDocument(BridgePolicyFor(browser_),
                                               frame->GetURL())) {
    return;
  }
  auto event = CefProcessMessage::Create("sabine.event");
  event->GetArgumentList()->SetString(0, JsString(name));
  event->GetArgumentList()->SetString(1, payload.empty() ? "null" : payload);
  frame->SendProcessMessage(PID_RENDERER, event);
}
