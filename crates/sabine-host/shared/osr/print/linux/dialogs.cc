#include "osr/print/linux/dialogs.h"

#include <cmath>
#include <cstdlib>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/strings.h"
#include "osr/handler.h"

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

void PrintDialogs::OnPrintSettings(CefRefPtr<CefBrowser> browser,
                                   CefRefPtr<CefPrintSettings> settings,
                                   bool get_defaults) {
  CEF_REQUIRE_UI_THREAD();
  ApplyPaper(settings, paper_);
}

bool PrintDialogs::OnPrintDialog(CefRefPtr<CefBrowser> browser,
                                 bool has_selection,
                                 CefRefPtr<CefPrintDialogCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const uint32_t id = ++serial_;
  const std::string payload =
      "{\"id\":" + std::to_string(id) +
      ",\"browser\":" + std::to_string(browser->GetIdentifier()) +
      ",\"hasSelection\":" + (has_selection ? "true" : "false") + "}";
  if (!owner_->SendMessage(kPrintDialog, {payload})) {
    return false;
  }
  dialogs_[id] = callback;
  return true;
}

void PrintDialogs::FinishDialog(const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const auto dialog = dialogs_.find(
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10)));
  if (dialog == dialogs_.end()) {
    return;
  }
  CefRefPtr<CefPrintDialogCallback> callback = dialog->second;
  dialogs_.erase(dialog);
  if (parts.size() != 7) {
    callback->Cancel();
    return;
  }
  paper_ = {std::strtod(parts[3].c_str(), nullptr),
            std::strtod(parts[4].c_str(), nullptr), parts[5] == "1"};
  CefRefPtr<CefPrintSettings> settings = CefPrintSettings::Create();
  ApplyPaper(settings, paper_);
  settings->SetPageRanges(PageRanges(parts[2]));
  settings->SetSelectionOnly(parts[6] == "1");
  callback->Continue(settings);
}

bool PrintDialogs::OnPrintJob(CefRefPtr<CefBrowser> browser,
                              const CefString& document_name,
                              const CefString& pdf_file_path,
                              CefRefPtr<CefPrintJobCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const uint32_t id = ++serial_;
  const std::string payload =
      "{\"id\":" + std::to_string(id) +
      ",\"browser\":" + std::to_string(browser->GetIdentifier()) +
      ",\"title\":" + JsString(document_name.ToString()) +
      ",\"path\":" + JsString(pdf_file_path.ToString()) + "}";
  if (!owner_->SendMessage(kPrintJob, {payload})) {
    return false;
  }
  jobs_[id] = callback;
  return true;
}

void PrintDialogs::FinishJob(const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const auto job = jobs_.find(
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10)));
  if (job == jobs_.end()) {
    return;
  }
  CefRefPtr<CefPrintJobCallback> callback = job->second;
  jobs_.erase(job);
  callback->Continue();
}
