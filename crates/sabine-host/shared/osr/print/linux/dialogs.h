#ifndef SABINE_CEF_HOST_OSR_PRINT_LINUX_DIALOGS_H_
#define SABINE_CEF_HOST_OSR_PRINT_LINUX_DIALOGS_H_

#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include "include/cef_print_handler.h"

class SabineOsrHandler;

struct PrintPaper {
  double width_mm = 210;
  double height_mm = 297;
  bool landscape = false;
};

// Chromium has no print dialog of its own on Linux, so a page's print dialog
// and the job it prints go to the window, which shows the desktop's.
class PrintDialogs : public CefPrintHandler {
 public:
  explicit PrintDialogs(SabineOsrHandler* owner) : owner_(owner) {}

  void OnPrintStart(CefRefPtr<CefBrowser> browser) override {}
  void OnPrintSettings(CefRefPtr<CefBrowser> browser,
                       CefRefPtr<CefPrintSettings> settings,
                       bool get_defaults) override;
  bool OnPrintDialog(CefRefPtr<CefBrowser> browser,
                     bool has_selection,
                     CefRefPtr<CefPrintDialogCallback> callback) override;
  bool OnPrintJob(CefRefPtr<CefBrowser> browser,
                  const CefString& document_name,
                  const CefString& pdf_file_path,
                  CefRefPtr<CefPrintJobCallback> callback) override;
  void OnPrintReset(CefRefPtr<CefBrowser> browser) override {}

  void FinishDialog(const std::vector<std::string>& parts);
  void FinishJob(const std::vector<std::string>& parts);

 private:
  SabineOsrHandler* const owner_;
  PrintPaper paper_;
  uint32_t serial_ = 0;
  std::map<uint32_t, CefRefPtr<CefPrintDialogCallback>> dialogs_;
  std::map<uint32_t, CefRefPtr<CefPrintJobCallback>> jobs_;

  IMPLEMENT_REFCOUNTING(PrintDialogs);
};

#endif
