#ifndef SABINE_CEF_HOST_OSR_DIALOGS_DIALOGS_H_
#define SABINE_CEF_HOST_OSR_DIALOGS_DIALOGS_H_

#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include "include/cef_context_menu_handler.h"
#include "include/cef_dialog_handler.h"
#include "include/cef_jsdialog_handler.h"

class SabineOsrHandler;

// The context menus, file dialogs and JavaScript dialogs the window shows for
// its pages. Offscreen browsers have no window for Chromium to put its own
// alert, confirm, prompt and leave-page dialogs on, so the window draws them.
class PageDialogs : public CefContextMenuHandler,
                    public CefDialogHandler,
                    public CefJSDialogHandler {
 public:
  explicit PageDialogs(SabineOsrHandler* owner) : owner_(owner) {}

  void OnBeforeContextMenu(CefRefPtr<CefBrowser> browser,
                           CefRefPtr<CefFrame> frame,
                           CefRefPtr<CefContextMenuParams> params,
                           CefRefPtr<CefMenuModel> model) override;
  bool RunContextMenu(CefRefPtr<CefBrowser> browser,
                      CefRefPtr<CefFrame> frame,
                      CefRefPtr<CefContextMenuParams> params,
                      CefRefPtr<CefMenuModel> model,
                      CefRefPtr<CefRunContextMenuCallback> callback) override;
  bool OnContextMenuCommand(CefRefPtr<CefBrowser> browser,
                            CefRefPtr<CefFrame> frame,
                            CefRefPtr<CefContextMenuParams> params,
                            int command_id,
                            EventFlags event_flags) override;
  void OnContextMenuDismissed(CefRefPtr<CefBrowser> browser,
                              CefRefPtr<CefFrame> frame) override;
  bool OnFileDialog(CefRefPtr<CefBrowser> browser,
                    FileDialogMode mode,
                    const CefString& title,
                    const CefString& default_file_path,
                    const std::vector<CefString>& accept_filters,
                    const std::vector<CefString>& accept_extensions,
                    const std::vector<CefString>& accept_descriptions,
                    CefRefPtr<CefFileDialogCallback> callback) override;
  bool OnJSDialog(CefRefPtr<CefBrowser> browser,
                  const CefString& origin_url,
                  JSDialogType dialog_type,
                  const CefString& message_text,
                  const CefString& default_prompt_text,
                  CefRefPtr<CefJSDialogCallback> callback,
                  bool& suppress_message) override;
  bool OnBeforeUnloadDialog(CefRefPtr<CefBrowser> browser,
                            const CefString& message_text,
                            bool is_reload,
                            CefRefPtr<CefJSDialogCallback> callback) override;
  void OnResetDialogState(CefRefPtr<CefBrowser> browser) override;

  void ChooseContextMenuCommand(int command_id);
  // Whether the app adds items of its own to the page's context menu, which
  // then opens even where Chromium offers none. Guests' menus keep only
  // Chromium's.
  void SetAppContextItems(bool present) { app_context_items_ = present; }
  void FinishFileDialog(const std::vector<std::string>& parts);
  void FinishJsDialog(const std::vector<std::string>& parts);

 private:
  struct JsDialog {
    int browser_id;
    CefRefPtr<CefJSDialogCallback> callback;
  };

  bool ShowJsDialog(CefRefPtr<CefBrowser> browser,
                    const std::string& fields,
                    CefRefPtr<CefJSDialogCallback> callback);

  SabineOsrHandler* const owner_;
  CefRefPtr<CefRunContextMenuCallback> context_menu_;
  bool app_context_items_ = false;
  uint32_t file_serial_ = 0;
  std::map<uint32_t, CefRefPtr<CefFileDialogCallback>> files_;
  uint32_t js_dialog_serial_ = 0;
  std::map<uint32_t, JsDialog> js_dialogs_;

  IMPLEMENT_REFCOUNTING(PageDialogs);
};

#endif
