#include "osr/handler.h"

#include <string>

#include "common/bridge_policy.h"
#include "include/cef_parser.h"
#include "include/wrapper/cef_helpers.h"

namespace {
constexpr size_t kMaxPendingClipboardRequests = 64;

void ReplyToPage(CefRefPtr<CefFrame> frame,
                 int page_request,
                 bool ok,
                 const std::string& payload) {
  auto message = CefProcessMessage::Create("sabine.clipboard");
  message->GetArgumentList()->SetString(
      0, "{\"id\":" + std::to_string(page_request) +
             ",\"ok\":" + (ok ? "true" : "false") +
             ",\"value\":" + (payload.empty() ? "null" : payload) + "}");
  frame->SendProcessMessage(PID_RENDERER, message);
}
}  // namespace

void SabineOsrHandler::ForwardClipboardRequest(CefRefPtr<CefBrowser> browser,
                                               CefRefPtr<CefFrame> frame,
                                               const std::string& request) {
  CEF_REQUIRE_UI_THREAD();
  auto value = CefParseJSON(request, JSON_PARSER_RFC);
  auto body = value ? value->GetDictionary() : nullptr;
  if (!body || body->GetType("id") != VTYPE_INT)
    return;
  const std::string operation = body->GetString("op");
  if (operation != "read" && operation != "write")
    return;
  if (clipboard_requests_.size() >= kMaxPendingClipboardRequests) {
    ReplyToPage(frame, body->GetInt("id"), false,
                "{\"message\":\"Too many clipboard requests are waiting\"}");
    return;
  }
  const std::string url = frame->GetURL();
  const std::string request_id =
      "clipboard-" + std::to_string(++clipboard_request_serial_);
  clipboard_requests_[request_id] = {browser, frame->GetIdentifier(),
                                     body->GetInt("id")};
  body->Remove("id");
  body->Remove("op");
  body->SetBool("trusted",
                sabine_bridge::AllowsDocument(BridgePolicyFor(browser), url));
  auto payload = CefValue::Create();
  payload->SetDictionary(body);
  const std::string line =
      "SABINE_BRIDGE_REQUEST\t" + std::to_string(browser->GetIdentifier()) +
      "\t" + request_id + "\t" + sabine_bridge::Origin(url) + "\t" +
      kClipboardCommandPrefix + operation + "\t" +
      CefWriteJSON(payload, JSON_WRITER_DEFAULT).ToString();
  SendMessage(kBridgeRequest, 0, 0, 0, 0, line.data(),
              static_cast<uint32_t>(line.size()));
}

bool SabineOsrHandler::ResolveClipboardResponse(const std::string& request_id,
                                                bool ok,
                                                const std::string& payload) {
  auto found = clipboard_requests_.find(request_id);
  if (found == clipboard_requests_.end())
    return false;
  const ClipboardRequest request = found->second;
  clipboard_requests_.erase(found);
  auto frame = request.browser->GetFrameByIdentifier(request.frame);
  if (frame && frame->IsValid())
    ReplyToPage(frame, request.page_request, ok, payload);
  return true;
}
