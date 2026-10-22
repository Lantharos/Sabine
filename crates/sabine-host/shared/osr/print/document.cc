#include "osr/print/document.h"

#include <utility>

#include "include/wrapper/cef_helpers.h"

namespace sabine_osr {

namespace {

constexpr double kMillimetersPerInch = 25.4;
constexpr int kViewWidth = 816;
constexpr int kViewHeight = 1056;

}  // namespace

PrintDocument::PrintDocument(PrintDocumentRequest request,
                             Finished finished,
                             Closed closed)
    : request_(std::move(request)),
      finished_(std::move(finished)),
      closed_(std::move(closed)) {}

bool PrintDocument::Open(CefWindowHandle parent,
                         CefRefPtr<CefRequestContext> context) {
  CEF_REQUIRE_UI_THREAD();
  CefWindowInfo window_info;
  window_info.SetAsWindowless(parent);
  CefBrowserSettings settings;
  settings.windowless_frame_rate = 1;
  return CefBrowserHost::CreateBrowser(window_info, this, request_.url,
                                       settings, nullptr, context);
}

void PrintDocument::Close() {
  CEF_REQUIRE_UI_THREAD();
  closing_ = true;
  if (browser_) {
    browser_->GetHost()->CloseBrowser(true);
  }
}

void PrintDocument::OnAfterCreated(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  browser_ = browser;
  if (closing_) {
    browser->GetHost()->CloseBrowser(true);
  }
}

void PrintDocument::OnBeforeClose(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  browser_ = nullptr;
  Finish(false);
  closed_(request_.id);
}

void PrintDocument::OnLoadingStateChange(CefRefPtr<CefBrowser> browser,
                                         bool is_loading,
                                         bool can_go_back,
                                         bool can_go_forward) {
  CEF_REQUIRE_UI_THREAD();
  if (is_loading || started_ || closing_) {
    return;
  }
  started_ = true;
  if (PrintsThroughDialog()) {
    browser->GetHost()->Print();
    Finish(true);
    return;
  }
  CefPdfPrintSettings settings;
  settings.paper_width = request_.width_mm / kMillimetersPerInch;
  settings.paper_height = request_.height_mm / kMillimetersPerInch;
  settings.landscape = request_.landscape;
  settings.print_background = true;
  browser->GetHost()->PrintToPDF(request_.output, settings, this);
}

void PrintDocument::OnLoadError(CefRefPtr<CefBrowser> browser,
                                CefRefPtr<CefFrame> frame,
                                ErrorCode error_code,
                                const CefString& error_text,
                                const CefString& failed_url) {
  CEF_REQUIRE_UI_THREAD();
  if (error_code == ERR_ABORTED || !frame->IsMain()) {
    return;
  }
  started_ = true;
  Finish(false);
  Close();
}

void PrintDocument::GetViewRect(CefRefPtr<CefBrowser> browser, CefRect& rect) {
  rect = CefRect(0, 0, kViewWidth, kViewHeight);
}

void PrintDocument::OnPdfPrintFinished(const CefString& path, bool ok) {
  CEF_REQUIRE_UI_THREAD();
  Finish(ok);
  Close();
}

void PrintDocument::Finish(bool ok) {
  if (finished_reported_) {
    return;
  }
  finished_reported_ = true;
  finished_(request_.id, ok);
}

}  // namespace sabine_osr
