#include "app/clipboard.h"

#include <string>
#include <vector>

#include "sabine_clipboard_js.h"

namespace sabine_clipboard {
namespace {
constexpr size_t kMaxMessageBytes = 64 * 1024 * 1024;

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
    if (arguments.size() != 1 || !arguments[0]->IsString()) {
      exception = "Malformed clipboard request";
      return true;
    }
    const std::string request = arguments[0]->GetStringValue();
    if (request.size() > kMaxMessageBytes) {
      exception = "Clipboard data exceeds 64 MiB";
      return true;
    }
    if (!frame_->IsValid())
      return true;
    auto message = CefProcessMessage::Create("sabine.clipboard");
    message->GetArgumentList()->SetString(0, request);
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
  if (message->GetName() != "sabine.clipboard")
    return false;
  auto arguments = message->GetArgumentList();
  auto context = frame->GetV8Context();
  if (arguments->GetSize() != 1 || !context || !context->IsValid())
    return true;
  for (const auto& entry : installed) {
    if (entry.context->IsSame(context)) {
      entry.settle->ExecuteFunctionWithContext(
          context, nullptr,
          {CefV8Value::CreateString(arguments->GetString(0))});
      break;
    }
  }
  return true;
}
}  // namespace sabine_clipboard
