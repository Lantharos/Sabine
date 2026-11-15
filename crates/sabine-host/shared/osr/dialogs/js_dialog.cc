#include "osr/dialogs/dialogs.h"

#include <cstdlib>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/strings.h"
#include "osr/handler.h"

using namespace sabine_osr;

namespace {

const char* KindName(cef_jsdialog_type_t type) {
  switch (type) {
    case JSDIALOGTYPE_CONFIRM:
      return "confirm";
    case JSDIALOGTYPE_PROMPT:
      return "prompt";
    default:
      return "alert";
  }
}

}  // namespace

bool PageDialogs::OnJSDialog(CefRefPtr<CefBrowser> browser,
                             const CefString& origin_url,
                             JSDialogType dialog_type,
                             const CefString& message_text,
                             const CefString& default_prompt_text,
                             CefRefPtr<CefJSDialogCallback> callback,
                             bool& suppress_message) {
  CEF_REQUIRE_UI_THREAD();
  return ShowJsDialog(
      browser,
      std::string("\"kind\":\"") + KindName(dialog_type) +
          "\",\"message\":" + JsString(message_text.ToString()) +
          ",\"value\":" + JsString(default_prompt_text.ToString()) +
          ",\"origin\":" + JsString(origin_url.ToString()),
      callback);
}

bool PageDialogs::OnBeforeUnloadDialog(
    CefRefPtr<CefBrowser> browser,
    const CefString& message_text,
    bool is_reload,
    CefRefPtr<CefJSDialogCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  return ShowJsDialog(browser,
                      std::string("\"kind\":\"") +
                          (is_reload ? "beforereload" : "beforeunload") + "\"",
                      callback);
}

bool PageDialogs::ShowJsDialog(CefRefPtr<CefBrowser> browser,
                               const std::string& fields,
                               CefRefPtr<CefJSDialogCallback> callback) {
  const uint32_t id = ++js_dialog_serial_;
  const bool guest = owner_->guests_.ForBrowser(browser) != nullptr;
  const std::string payload = "{\"id\":" + std::to_string(id) + "," + fields +
                              ",\"guest\":" + (guest ? "true" : "false") + "}";
  if (!owner_->transport_.Send(kJsDialog, 0, 0, 0, 0, payload.data(),
                               static_cast<uint32_t>(payload.size()))) {
    return false;
  }
  js_dialogs_[id] = {browser->GetIdentifier(), callback};
  return true;
}

void PageDialogs::OnResetDialogState(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  for (auto dialog = js_dialogs_.begin(); dialog != js_dialogs_.end();) {
    if (dialog->second.browser_id != browser->GetIdentifier()) {
      ++dialog;
      continue;
    }
    const std::string payload =
        "{\"id\":" + std::to_string(dialog->first) + "}";
    owner_->transport_.Send(kJsDialogClosed, 0, 0, 0, 0, payload.data(),
                            static_cast<uint32_t>(payload.size()));
    dialog = js_dialogs_.erase(dialog);
  }
}

void PageDialogs::FinishJsDialog(const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const auto dialog = js_dialogs_.find(
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10)));
  if (dialog == js_dialogs_.end()) {
    return;
  }
  CefRefPtr<CefJSDialogCallback> callback = dialog->second.callback;
  js_dialogs_.erase(dialog);
  callback->Continue(parts[2] == "1", parts.size() > 3
                                          ? DecodeControlComponent(parts[3])
                                          : std::string());
}
