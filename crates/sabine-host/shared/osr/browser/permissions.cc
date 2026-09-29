#include "osr/handler.h"

#include "common/bridge_policy.h"
#include "include/wrapper/cef_helpers.h"

bool SabineOsrHandler::OnShowPermissionPrompt(
    CefRefPtr<CefBrowser> browser,
    uint64_t,
    const CefString& requesting_origin,
    uint32_t requested_permissions,
    CefRefPtr<CefPermissionPromptCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const bool app_clipboard =
      requested_permissions == CEF_PERMISSION_TYPE_CLIPBOARD &&
      sabine_bridge::AllowsDocument(BridgePolicyFor(browser),
                                    requesting_origin);
  callback->Continue(app_clipboard ? CEF_PERMISSION_RESULT_ACCEPT
                                   : CEF_PERMISSION_RESULT_DENY);
  return true;
}
