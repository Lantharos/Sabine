#ifndef SABINE_CEF_HOST_OSR_DIALOGS_DIALOGS_H_
#define SABINE_CEF_HOST_OSR_DIALOGS_DIALOGS_H_

#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include "include/cef_context_menu_handler.h"
#include "include/cef_dialog_handler.h"

class SabineOsrHandler;

// The context menus and file dialogs the window shows for its pages.
class PageDialogs : public CefContextMenuHandler, public CefDialogHandler {
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

  void ChooseContextMenuCommand(int command_id);
  void FinishFileDialog(const std::vector<std::string>& parts);

 private:
  SabineOsrHandler* const owner_;
  CefRefPtr<CefRunContextMenuCallback> context_menu_;
  uint32_t file_serial_ = 0;
  std::map<uint32_t, CefRefPtr<CefFileDialogCallback>> files_;

  IMPLEMENT_REFCOUNTING(PageDialogs);
};

#endif
