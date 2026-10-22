#include "osr/handler.h"

#if defined(OS_LINUX)

#include <cmath>
#include <cstdlib>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/strings.h"

using namespace sabine_osr;

namespace {

constexpr int kPrintDpi = 300;
constexpr double kMillimetersPerInch = 25.4;

int DeviceUnits(double millimeters) {
  return static_cast<int>(
      std::lround(millimeters / kMillimetersPerInch * kPrintDpi));
}

void ApplyPaper(CefRefPtr<CefPrintSettings> settings, const PrintPaper& paper) {
  const CefSize size(DeviceUnits(paper.width_mm), DeviceUnits(paper.height_mm));
  settings->SetDPI(kPrintDpi);
  settings->SetOrientation(paper.landscape);
  settings->SetPrinterPrintableArea(
      size, CefRect(0, 0, size.width, size.height), true);
}

CefPrintSettings::PageRangeList PageRanges(const std::string& ranges) {
  CefPrintSettings::PageRangeList list;
  for (const std::string& range : Split(ranges, ',')) {
    const auto bounds = Split(range, '-');
    if (bounds.size() == 2) {
      list.emplace_back(std::atoi(bounds[0].c_str()),
                        std::atoi(bounds[1].c_str()));
    }
  }
  return list;
}

}  // namespace

void SabineOsrHandler::OnPrintSettings(CefRefPtr<CefBrowser> browser,
                                       CefRefPtr<CefPrintSettings> settings,
                                       bool get_defaults) {
  CEF_REQUIRE_UI_THREAD();
  ApplyPaper(settings, print_paper_);
}

bool SabineOsrHandler::OnPrintDialog(
    CefRefPtr<CefBrowser> browser,
    bool has_selection,
    CefRefPtr<CefPrintDialogCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const uint32_t id = ++print_serial_;
  const std::string payload =
      "{\"id\":" + std::to_string(id) +
      ",\"browser\":" + std::to_string(browser->GetIdentifier()) +
      ",\"hasSelection\":" + (has_selection ? "true" : "false") + "}";
  if (!SendMessage(kPrintDialog, 0, 0, 0, 0, payload.data(),
                   static_cast<uint32_t>(payload.size()))) {
    return false;
  }
  print_dialogs_[id] = callback;
  return true;
}

void SabineOsrHandler::FinishPrintDialog(
    const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const auto dialog = print_dialogs_.find(
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10)));
  if (dialog == print_dialogs_.end()) {
    return;
  }
  CefRefPtr<CefPrintDialogCallback> callback = dialog->second;
  print_dialogs_.erase(dialog);
  if (parts.size() != 7) {
    callback->Cancel();
    return;
  }
  print_paper_ = {std::strtod(parts[3].c_str(), nullptr),
                  std::strtod(parts[4].c_str(), nullptr), parts[5] == "1"};
  CefRefPtr<CefPrintSettings> settings = CefPrintSettings::Create();
  ApplyPaper(settings, print_paper_);
  settings->SetPageRanges(PageRanges(parts[2]));
  settings->SetSelectionOnly(parts[6] == "1");
  callback->Continue(settings);
}

bool SabineOsrHandler::OnPrintJob(CefRefPtr<CefBrowser> browser,
                                  const CefString& document_name,
                                  const CefString& pdf_file_path,
                                  CefRefPtr<CefPrintJobCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const uint32_t id = ++print_serial_;
  const std::string payload =
      "{\"id\":" + std::to_string(id) +
      ",\"browser\":" + std::to_string(browser->GetIdentifier()) +
      ",\"title\":" + JsString(document_name.ToString()) +
      ",\"path\":" + JsString(pdf_file_path.ToString()) + "}";
  if (!SendMessage(kPrintJob, 0, 0, 0, 0, payload.data(),
                   static_cast<uint32_t>(payload.size()))) {
    return false;
  }
  print_jobs_[id] = callback;
  return true;
}

void SabineOsrHandler::FinishPrintJob(const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const auto job = print_jobs_.find(
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10)));
  if (job == print_jobs_.end()) {
    return;
  }
  CefRefPtr<CefPrintJobCallback> callback = job->second;
  print_jobs_.erase(job);
  callback->Continue();
}

#endif
