#include "renderer/clipboard.h"

#include <string>
#include <vector>

#include "common/bytes_message.h"
#include "sabine_clipboard_js.h"

namespace sabine_clipboard {
namespace {
constexpr size_t kMaxMessageBytes = 1024 * 1024;

struct Installed {
  CefRefPtr<CefV8Context> context;
  CefRefPtr<CefV8Value> settle;
};
std::vector<Installed> installed;

class Transport : public CefV8Handler {
 public:
  explicit Transport(CefRefPtr<CefFrame> frame) : frame_(frame) {}

  bool Execute(const CefString& name,
               CefRefPtr<CefV8Value> object,
               const CefV8ValueList& arguments,
               CefRefPtr<CefV8Value>& retval,
               CefString& exception) override {
    const bool carries_bytes =
        arguments.size() == 2 && arguments[1]->IsArrayBuffer();
    if ((arguments.size() != 1 && !carries_bytes) ||
        !arguments[0]->IsString()) {
      exception = "Malformed clipboard request";
      return true;
    }
    const std::string request = arguments[0]->GetStringValue();
    if (request.size() > kMaxMessageBytes) {
      exception = "Clipboard request exceeds 1 MiB";
      return true;
    }
    if (carries_bytes && arguments[1]->GetArrayBufferByteLength() >
                             sabine_bytes::kMaxBodyBytes) {
      exception = "Clipboard data exceeds 32 MiB";
      return true;
    }
    if (!frame_->IsValid())
      return true;
    CefRefPtr<CefProcessMessage> message;
    if (carries_bytes) {
      message = sabine_bytes::Create(
          "sabine.clipboard.bytes", {request},
          static_cast<const char*>(arguments[1]->GetArrayBufferData()),
          arguments[1]->GetArrayBufferByteLength());
    } else {
      message = CefProcessMessage::Create("sabine.clipboard");
      message->GetArgumentList()->SetString(0, request);
    }
    if (message)
      frame_->SendProcessMessage(PID_BROWSER, message);
    retval = CefV8Value::CreateUndefined();
    return true;
  }

 private:
  CefRefPtr<CefFrame> frame_;
  IMPLEMENT_REFCOUNTING(Transport);
};
}  // namespace

void Install(CefRefPtr<CefFrame> frame,
             CefRefPtr<CefV8Context> context,
             bool trusted) {
  CefRefPtr<CefV8Value> installer;
  CefRefPtr<CefV8Exception> exception;
  if (!context->Eval(SABINE_CLIPBOARD_JS_RAW, frame->GetURL(), 0, installer,
                     exception) ||
      !installer->IsFunction())
    return;
  auto settle = installer->ExecuteFunctionWithContext(
      context, nullptr,
      {CefV8Value::CreateFunction("post", new Transport(frame)),
       CefV8Value::CreateBool(trusted)});
  if (settle && settle->IsFunction())
    installed.push_back({context, settle});
}

void Release(CefRefPtr<CefV8Context> context) {
  for (auto it = installed.begin(); it != installed.end();) {
    if (it->context->IsSame(context))
      it = installed.erase(it);
    else
      ++it;
  }
}

bool Receive(CefRefPtr<CefFrame> frame, CefRefPtr<CefProcessMessage> message) {
  const std::string name = message->GetName();
  if (name != "sabine.clipboard" && name != "sabine.clipboard.bytes")
    return false;
  sabine_bytes::Message bytes;
  if (name == "sabine.clipboard.bytes"
          ? !sabine_bytes::Read(message, &bytes) || bytes.fields.size() != 1
          : message->GetArgumentList()->GetSize() != 1)
    return true;
  auto context = frame->GetV8Context();
  if (!context || !context->IsValid())
    return true;
  for (const auto& entry : installed) {
    if (!entry.context->IsSame(context))
      continue;
    if (!context->Enter())
      return true;
    CefV8ValueList arguments;
    if (bytes.region) {
      arguments = {CefV8Value::CreateString(bytes.fields[0]),
                   CefV8Value::CreateArrayBufferWithCopy(
                       const_cast<char*>(bytes.body), bytes.body_size)};
    } else {
      arguments = {
          CefV8Value::CreateString(message->GetArgumentList()->GetString(0))};
    }
    entry.settle->ExecuteFunction(nullptr, arguments);
    context->Exit();
    break;
  }
  return true;
}
}  // namespace sabine_clipboard
