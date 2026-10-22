#ifndef SABINE_CEF_HOST_OSR_PRINT_DOCUMENT_H_
#define SABINE_CEF_HOST_OSR_PRINT_DOCUMENT_H_

#include <cstdint>
#include <functional>
#include <string>

#include "include/cef_browser.h"
#include "include/cef_client.h"
#include "include/cef_life_span_handler.h"
#include "include/cef_load_handler.h"
#include "include/cef_render_handler.h"
#include "include/cef_request_context.h"

namespace sabine_osr {

struct PrintDocumentRequest {
  uint32_t id = 0;
  std::string url;
  // The PDF file to render the document into. Empty hands the document to
  // the system's print dialog instead.
  std::string output;
  double width_mm = 0;
  double height_mm = 0;
  bool landscape = false;
};

// A document loaded out of sight to be printed: rendered to a PDF file, or
// printed through the system's own dialog.
class PrintDocument : public CefClient,
                      public CefLifeSpanHandler,
                      public CefLoadHandler,
                      public CefRenderHandler,
                      public CefPdfPrintCallback {
 public:
  using Finished = std::function<void(uint32_t id, bool ok)>;
  using Closed = std::function<void(uint32_t id)>;

  PrintDocument(PrintDocumentRequest request, Finished finished, Closed closed);

  bool Open(CefWindowHandle parent, CefRefPtr<CefRequestContext> context);
  void Close();
  bool PrintsThroughDialog() const { return request_.output.empty(); }

  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }
  CefRefPtr<CefRenderHandler> GetRenderHandler() override { return this; }

  void OnAfterCreated(CefRefPtr<CefBrowser> browser) override;
  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;
  void OnLoadingStateChange(CefRefPtr<CefBrowser> browser,
                            bool is_loading,
                            bool can_go_back,
                            bool can_go_forward) override;
  void OnLoadError(CefRefPtr<CefBrowser> browser,
                   CefRefPtr<CefFrame> frame,
                   ErrorCode error_code,
                   const CefString& error_text,
                   const CefString& failed_url) override;
  void GetViewRect(CefRefPtr<CefBrowser> browser, CefRect& rect) override;
  void OnPaint(CefRefPtr<CefBrowser> browser,
               PaintElementType type,
               const RectList& dirty_rects,
               const void* buffer,
               int width,
               int height) override {}
  void OnPdfPrintFinished(const CefString& path, bool ok) override;

 private:
  void Finish(bool ok);

  PrintDocumentRequest request_;
  Finished finished_;
  Closed closed_;
  CefRefPtr<CefBrowser> browser_;
  bool started_ = false;
  bool finished_reported_ = false;
  bool closing_ = false;

  IMPLEMENT_REFCOUNTING(PrintDocument);
};

}  // namespace sabine_osr

#endif
