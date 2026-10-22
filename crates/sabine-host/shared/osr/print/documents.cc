#include "osr/handler.h"

#include <cstdlib>
#include <string>
#include <vector>

#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

void SabineOsrHandler::OpenPrintDocument(
    const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  PrintDocumentRequest request;
  request.id =
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10));
  request.url = parts[2];
  request.output = parts[3];
  request.width_mm = std::strtod(parts[4].c_str(), nullptr);
  request.height_mm = std::strtod(parts[5].c_str(), nullptr);
  request.landscape = parts[6] == "1";
  if (!browser_ || closing_) {
    ReportPrintDocument(request.id, false);
    return;
  }
  if (request.output.empty()) {
    for (const auto& [id, document] : print_documents_) {
      if (document->PrintsThroughDialog()) {
        document->Close();
      }
    }
  }
  CefRefPtr<SabineOsrHandler> self(this);
  CefRefPtr<PrintDocument> document(new PrintDocument(
      request,
      [self](uint32_t id, bool ok) { self->ReportPrintDocument(id, ok); },
      [self](uint32_t id) { self->PrintDocumentClosed(id); }));
  if (!document->Open(browser_->GetHost()->GetWindowHandle(),
                      profile_.context)) {
    ReportPrintDocument(request.id, false);
    return;
  }
  print_documents_[request.id] = document;
}

void SabineOsrHandler::ReportPrintDocument(uint32_t id, bool ok) {
  const std::string payload = "{\"id\":" + std::to_string(id) +
                              ",\"ok\":" + (ok ? "true" : "false") + "}";
  SendMessage(kPrintDocument, 0, 0, 0, 0, payload.data(),
              static_cast<uint32_t>(payload.size()));
}

void SabineOsrHandler::PrintDocumentClosed(uint32_t id) {
  CEF_REQUIRE_UI_THREAD();
  print_documents_.erase(id);
  QuitWhenClosed();
}

void SabineOsrHandler::ClosePrintDocuments() {
  for (const auto& [id, document] : print_documents_) {
    document->Close();
  }
}
