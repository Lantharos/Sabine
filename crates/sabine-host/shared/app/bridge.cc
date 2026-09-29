#include "app/bridge.h"
#include <functional>
#include <map>
#include <utility>
#include <vector>
#include "common/bridge_policy.h"
#include "common/bytes_message.h"
#include "osr/utilities.h"

namespace sabine_bridge {
namespace {
struct PendingRequest {
  CefRefPtr<CefV8Context> context;
  std::string page_id;
  CefRefPtr<CefBrowser> browser;
};
std::map<std::string, PendingRequest> pending;
struct ContextOrigin {
  CefRefPtr<CefV8Context> context;
  std::string origin;
};
std::vector<ContextOrigin> contexts;
const std::string process_token = UniqueToken();
uint64_t next_request = 0;
constexpr size_t kMaxPendingRequests = 128;
constexpr size_t kMaxMessageBytes = 1024 * 1024;

class NativePostMessageHandler : public CefV8Handler {
 public:
  NativePostMessageHandler(CefRefPtr<CefFrame> frame,
                           std::string message_name,
                           std::string origin)
      : frame_(frame),
        message_name_(std::move(message_name)),
        origin_(std::move(origin)) {}

  bool Execute(const CefString& name,
               CefRefPtr<CefV8Value> object,
               const CefV8ValueList& arguments,
               CefRefPtr<CefV8Value>& retval,
               CefString& exception) override {
    std::vector<std::string> values;
    size_t bytes = 0;
    CefRefPtr<CefV8Value> body;
    for (size_t index = 0; index < arguments.size(); ++index) {
      const auto& argument = arguments[index];
      if (index + 1 == arguments.size() && argument->IsArrayBuffer()) {
        body = argument;
        continue;
      }
      if (!argument->IsString()) {
        exception = "Sabine native messages take string arguments";
        return true;
      }
      values.push_back(argument->GetStringValue());
      bytes += values.back().size();
    }
    if (values.empty()) {
      exception = "Sabine native messages need at least one argument";
      return true;
    }
    if (bytes > kMaxMessageBytes) {
      exception = "Sabine native message exceeds 1 MiB";
      return true;
    }
    auto context = CefV8Context::GetCurrentContext();
    if (!context || !context->IsValid() || !frame_->IsValid())
      return true;
    const bool native = message_name_ == "sabine.native";
    if (native && !TrackRequest(context, &values, exception))
      return true;
    if (body) {
      if (!native || values.front() != "bridge" ||
          body->GetArrayBufferByteLength() > sabine_bytes::kMaxBodyBytes) {
        exception = "Sabine bridge bytes are limited to 32 MiB per call";
        return true;
      }
      values.insert(values.begin(), origin_);
      auto message = sabine_bytes::Create(
          "sabine.native.bytes", values,
          static_cast<const char*>(body->GetArrayBufferData()),
          body->GetArrayBufferByteLength());
      if (message)
        frame_->SendProcessMessage(PID_BROWSER, message);
      retval = CefV8Value::CreateUndefined();
      return true;
    }
    auto message = CefProcessMessage::Create(message_name_);
    auto list = message->GetArgumentList();
    size_t index = 0;
    if (native)
      list->SetString(index++, origin_);
    for (const auto& value : values)
      list->SetString(index++, value);
    frame_->SendProcessMessage(PID_BROWSER, message);
    retval = CefV8Value::CreateUndefined();
    return true;
  }

 private:
  bool TrackRequest(CefRefPtr<CefV8Context> context,
                    std::vector<std::string>* values,
                    CefString& exception) {
    const std::string& kind = values->front();
    auto browser = frame_->GetBrowser();
    if (kind == "cancel") {
      if (values->size() == 2)
        CancelRequest(browser, context, (*values)[1]);
      return false;
    }
    if (kind != "bridge")
      return true;
    if (values->size() != 4) {
      exception = "Malformed Sabine bridge request";
      return false;
    }
    if (pending.size() >= kMaxPendingRequests) {
      exception = "Sabine bridge request capacity is exhausted";
      return false;
    }
    const std::string native_id =
        process_token + "-" + std::to_string(++next_request);
    pending.emplace(native_id, PendingRequest{context, (*values)[1], browser});
    (*values)[1] = native_id;
    return true;
  }

  static void CancelRequest(CefRefPtr<CefBrowser> browser,
                            CefRefPtr<CefV8Context> context,
                            const std::string& page_id) {
    for (auto it = pending.begin(); it != pending.end();) {
      if (it->second.browser->IsSame(browser) &&
          it->second.page_id == page_id && it->second.context->IsSame(context))
        it = pending.erase(it);
      else
        ++it;
    }
  }

  CefRefPtr<CefFrame> frame_;
  std::string message_name_;
  std::string origin_;
  IMPLEMENT_REFCOUNTING(NativePostMessageHandler);
};

CefRefPtr<CefV8Value> ParseJson(CefRefPtr<CefV8Value> global,
                                const std::string& text) {
  auto json = global->GetValue("JSON");
  auto parse = json ? json->GetValue("parse") : nullptr;
  if (!parse || !parse->IsFunction())
    return CefV8Value::CreateNull();
  auto value = parse->ExecuteFunction(json, {CefV8Value::CreateString(text)});
  return value ? value : CefV8Value::CreateNull();
}

void CallPage(
    CefRefPtr<CefV8Context> context,
    const char* function_name,
    const std::function<CefV8ValueList(CefRefPtr<CefV8Value>)>& arguments) {
  if (!context || !context->IsValid() || !context->Enter())
    return;
  auto global = context->GetGlobal();
  auto function = global->GetValue(function_name);
  if (function && function->IsFunction())
    function->ExecuteFunction(nullptr, arguments(global));
  context->Exit();
}

CefRefPtr<CefV8Value> ArrayBuffer(const sabine_bytes::Message& bytes) {
  return CefV8Value::CreateArrayBufferWithCopy(const_cast<char*>(bytes.body),
                                               bytes.body_size);
}

bool AuthorizedEventContext(CefRefPtr<CefFrame> frame,
                            CefRefPtr<CefDictionaryValue> policy,
                            CefRefPtr<CefV8Context>* context) {
  if (!frame->IsMain() || !AllowsDocument(policy, frame->GetURL()))
    return false;
  *context = frame->GetV8Context();
  if (!*context || !(*context)->IsValid())
    return false;
  for (const auto& entry : contexts) {
    if (entry.context->IsSame(*context))
      return MatchesSecurityOrigin(policy, frame->GetURL(), entry.origin);
  }
  return false;
}

void ReceiveBytes(CefRefPtr<CefBrowser> browser,
                  CefRefPtr<CefFrame> frame,
                  bool event,
                  const sabine_bytes::Message& bytes,
                  CefRefPtr<CefDictionaryValue> policy) {
  if (event) {
    CefRefPtr<CefV8Context> context;
    if (!AuthorizedEventContext(frame, policy, &context))
      return;
    CallPage(context, "__sabineBridgeEmit",
             [&](CefRefPtr<CefV8Value> global) -> CefV8ValueList {
               return {ParseJson(global, bytes.fields[0]), ArrayBuffer(bytes)};
             });
    return;
  }
  auto found = pending.find(bytes.fields[0]);
  if (found == pending.end() || !found->second.browser->IsSame(browser))
    return;
  const PendingRequest request = found->second;
  pending.erase(found);
  CallPage(request.context, "__sabineBridgeResolve",
           [&](CefRefPtr<CefV8Value>) -> CefV8ValueList {
             return {CefV8Value::CreateString(request.page_id),
                     CefV8Value::CreateBool(true), ArrayBuffer(bytes)};
           });
}
}  // namespace

void InstallTransport(CefRefPtr<CefFrame> frame,
                      CefRefPtr<CefV8Context> context,
                      const char* name,
                      const char* message) {
  context->GetGlobal()->SetValue(
      name,
      CefV8Value::CreateFunction(
          name,
          new NativePostMessageHandler(
              frame, message,
              context->GetGlobal()->GetValue("origin")->GetStringValue())),
      static_cast<CefV8Value::PropertyAttribute>(
          V8_PROPERTY_ATTRIBUTE_READONLY | V8_PROPERTY_ATTRIBUTE_DONTDELETE));
}

std::string RememberContext(CefRefPtr<CefV8Context> context) {
  const std::string origin =
      context->GetGlobal()->GetValue("origin")->GetStringValue();
  contexts.push_back({context, origin});
  return origin;
}

void ReleaseContext(CefRefPtr<CefV8Context> context) {
  for (auto it = contexts.begin(); it != contexts.end();) {
    if (it->context->IsSame(context))
      it = contexts.erase(it);
    else
      ++it;
  }
  for (auto it = pending.begin(); it != pending.end();) {
    if (it->second.context->IsSame(context))
      it = pending.erase(it);
    else
      ++it;
  }
}

void ReleaseBrowser(CefRefPtr<CefBrowser> browser) {
  for (auto it = pending.begin(); it != pending.end();) {
    if (it->second.browser->IsSame(browser))
      it = pending.erase(it);
    else
      ++it;
  }
}

bool Receive(CefRefPtr<CefBrowser> browser,
             CefRefPtr<CefFrame> frame,
             CefRefPtr<CefProcessMessage> message,
             CefRefPtr<CefDictionaryValue> policy) {
  const std::string name = message->GetName();
  if (name == "sabine.response.bytes" || name == "sabine.event.bytes") {
    sabine_bytes::Message bytes;
    if (sabine_bytes::Read(message, &bytes) && bytes.fields.size() == 1)
      ReceiveBytes(browser, frame, name == "sabine.event.bytes", bytes, policy);
    return true;
  }
  auto values = message->GetArgumentList();
  if (name == "sabine.response") {
    if (values->GetSize() != 3)
      return true;
    auto found = pending.find(values->GetString(0));
    if (found == pending.end() || !found->second.browser->IsSame(browser))
      return true;
    const PendingRequest request = found->second;
    pending.erase(found);
    const bool ok = values->GetBool(1);
    const std::string payload = values->GetString(2);
    CallPage(request.context, "__sabineBridgeResolve",
             [&](CefRefPtr<CefV8Value> global) -> CefV8ValueList {
               return {CefV8Value::CreateString(request.page_id),
                       CefV8Value::CreateBool(ok), ParseJson(global, payload)};
             });
    return true;
  }
  if (name != "sabine.event")
    return false;
  CefRefPtr<CefV8Context> context;
  if (values->GetSize() != 2 ||
      !AuthorizedEventContext(frame, policy, &context))
    return true;
  const std::string event_name = values->GetString(0);
  const std::string payload = values->GetString(1);
  CallPage(context, "__sabineBridgeEmit",
           [&](CefRefPtr<CefV8Value> global) -> CefV8ValueList {
             return {ParseJson(global, event_name), ParseJson(global, payload)};
           });
  return true;
}
}  // namespace sabine_bridge
