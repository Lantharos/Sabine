#include "osr/page/clipboard.h"

#include <string>

#include "common/bridge_policy.h"
#include "common/bytes_message.h"
#include "include/cef_parser.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/transport/message_kinds.h"

namespace {
constexpr size_t kMaxPendingClipboardRequests = 64;

void ReplyToPage(CefRefPtr<CefFrame> frame,
                 int page_request,
                 bool ok,
                 const std::string& payload,
                 const std::optional<std::string>& body) {
  const std::string reply =
      "{\"id\":" + std::to_string(page_request) +
      ",\"ok\":" + (ok ? "true" : "false") +
      ",\"value\":" + (payload.empty() ? "null" : payload) + "}";
  CefRefPtr<CefProcessMessage> message;
  if (body) {
    message = sabine_bytes::Create("sabine.clipboard.bytes", {reply},
                                   body->data(), body->size());
  } else {
    message = CefProcessMessage::Create("sabine.clipboard");
    message->GetArgumentList()->SetString(0, reply);
  }
  if (message)
    frame->SendProcessMessage(PID_RENDERER, message);
}
}  // namespace

void ClipboardRequests::Forget(CefRefPtr<CefBrowser> browser) {
  for (auto it = pending_.begin(); it != pending_.end();) {
    if (it->second.browser->IsSame(browser))
      it = pending_.erase(it);
    else
      ++it;
  }
}

void ClipboardRequests::Forward(CefRefPtr<CefBrowser> browser,
                                CefRefPtr<CefFrame> frame,
                                const std::string& request,
                                std::optional<std::string_view> body,
                                CefRefPtr<CefDictionaryValue> policy) {
  CEF_REQUIRE_UI_THREAD();
  auto value = CefParseJSON(request, JSON_PARSER_RFC);
  auto fields = value ? value->GetDictionary() : nullptr;
  if (!fields || fields->GetType("id") != VTYPE_INT)
    return;
  const std::string operation = fields->GetString("op");
  if (operation != "read" && operation != "write")
    return;
  if (pending_.size() >= kMaxPendingClipboardRequests) {
    ReplyToPage(frame, fields->GetInt("id"), false,
                "{\"message\":\"Too many clipboard requests are waiting\"}",
                std::nullopt);
    return;
  }
  const std::string url = frame->GetURL();
  const std::string request_id = "clipboard-" + std::to_string(++serial_);
  pending_[request_id] = {browser, frame->GetIdentifier(),
                          fields->GetInt("id")};
  fields->Remove("id");
  fields->Remove("op");
  fields->SetBool("trusted", sabine_bridge::AllowsDocument(policy, url));
  auto payload = CefValue::Create();
  payload->SetDictionary(fields);
  const std::string line =
      "SABINE_BRIDGE_REQUEST\t" + std::to_string(browser->GetIdentifier()) +
      "\t" + request_id + "\t" + sabine_bridge::Origin(url) + "\t" +
      kClipboardCommandPrefix + operation + "\t" +
      CefWriteJSON(payload, JSON_WRITER_DEFAULT).ToString();
  if (!body) {
    transport_.Send(kBridgeRequest, {line});
    return;
  }
  const std::string header =
      sabine_bytes::kPrefix + std::to_string(body->size()) + "\t";
  transport_.Send(kBridgeRequest, {header, line, "\n", *body});
}

bool ClipboardRequests::Resolve(const std::string& request_id,
                                bool ok,
                                const std::string& payload,
                                const std::optional<std::string>& body) {
  auto found = pending_.find(request_id);
  if (found == pending_.end())
    return false;
  const Request request = found->second;
  pending_.erase(found);
  auto frame = request.browser->GetFrameByIdentifier(request.frame);
  if (frame && frame->IsValid())
    ReplyToPage(frame, request.page_request, ok, payload, body);
  return true;
}
