#include "osr/handler.h"

#include <algorithm>

#include "common/bridge_policy.h"
#include "include/wrapper/cef_helpers.h"

namespace {

bool Contains(const std::vector<std::string>& values,
              const std::string& value) {
  return std::find(values.begin(), values.end(), value) != values.end();
}

}  // namespace

bool SabineOsrHandler::OnRequestMediaAccessPermission(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame>,
    const CefString& requesting_origin,
    uint32_t requested_permissions,
    CefRefPtr<CefMediaAccessCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  AskPermissions(browser, 0, requesting_origin,
                 MediaPermissions(requested_permissions),
                 [callback, requested_permissions](bool allowed) {
                   if (allowed) {
                     callback->Continue(requested_permissions);
                   } else {
                     callback->Cancel();
                   }
                 });
  return true;
}

bool SabineOsrHandler::OnShowPermissionPrompt(
    CefRefPtr<CefBrowser> browser,
    uint64_t prompt_id,
    const CefString& requesting_origin,
    uint32_t requested_permissions,
    CefRefPtr<CefPermissionPromptCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const bool app_clipboard =
      requested_permissions == CEF_PERMISSION_TYPE_CLIPBOARD &&
      sabine_bridge::AllowsDocument(BridgePolicyFor(browser),
                                    requesting_origin);
  if (app_clipboard) {
    callback->Continue(CEF_PERMISSION_RESULT_ACCEPT);
    return true;
  }
  AskPermissions(browser, prompt_id, requesting_origin,
                 PromptPermissions(requested_permissions),
                 [callback](bool allowed) {
                   callback->Continue(allowed ? CEF_PERMISSION_RESULT_ACCEPT
                                              : CEF_PERMISSION_RESULT_DENY);
                 });
  return true;
}

void SabineOsrHandler::OnDismissPermissionPrompt(
    CefRefPtr<CefBrowser>,
    uint64_t prompt_id,
    cef_permission_request_result_t) {
  CEF_REQUIRE_UI_THREAD();
  permissions_.Dismiss(prompt_id);
}

// Only the app's own documents, and guests the app let ask, may ask, and only
// for permissions the app declares.
void SabineOsrHandler::AskPermissions(CefRefPtr<CefBrowser> browser,
                                      uint64_t prompt_id,
                                      const CefString& requesting_origin,
                                      std::vector<std::string> requested,
                                      PagePermissions::Answer answer) {
  const std::vector<std::string> declared =
      sabine_bridge::Strings(bridge_policy_, "permissions");
  const GuestView* guest = guests_.ForBrowser(browser);
  const bool app =
      !guest && browser_ && browser_->IsSame(browser) &&
      sabine_bridge::AllowsDocument(bridge_policy_, requesting_origin);
  const bool asks =
      !requested.empty() && (app || guest) &&
      std::all_of(requested.begin(), requested.end(),
                  [&](const std::string& permission) {
                    return Contains(declared, permission) &&
                           (!guest || Contains(guest->permissions, permission));
                  });
  if (!asks) {
    answer(false);
    return;
  }
  PermissionAsk ask;
  ask.browser_id = browser->GetIdentifier();
  ask.prompt_id = prompt_id;
  ask.origin = requesting_origin.ToString();
  ask.app = app;
  ask.guest = guest ? guest->id : std::string();
  ask.permissions = std::move(requested);
  permissions_.Ask(ask, std::move(answer));
}
