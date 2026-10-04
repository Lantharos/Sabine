#include "osr/handler.h"

#include <cstdlib>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/strings.h"

using namespace sabine_osr;

namespace {

const char* ModeName(cef_file_dialog_mode_t mode) {
  switch (mode) {
    case FILE_DIALOG_OPEN_MULTIPLE:
      return "openMultiple";
    case FILE_DIALOG_OPEN_FOLDER:
      return "openFolder";
    case FILE_DIALOG_SAVE:
      return "save";
    default:
      return "open";
  }
}

std::string FilterJson(const std::string& filter,
                       const std::string& extensions,
                       const std::string& description) {
  std::string list;
  for (std::string extension :
       Split(extensions.empty() ? filter : extensions, ';')) {
    if (extension.size() < 2 || extension[0] != '.') {
      continue;
    }
    list += (list.empty() ? "" : ",") + JsString(extension.substr(1));
  }
  if (list.empty()) {
    return std::string();
  }
  return "{\"description\":" +
         JsString(description.empty() ? filter : description) +
         ",\"extensions\":[" + list + "]}";
}

}  // namespace

bool SabineOsrHandler::OnFileDialog(
    CefRefPtr<CefBrowser> browser,
    FileDialogMode mode,
    const CefString& title,
    const CefString& default_file_path,
    const std::vector<CefString>& accept_filters,
    const std::vector<CefString>& accept_extensions,
    const std::vector<CefString>& accept_descriptions,
    CefRefPtr<CefFileDialogCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  std::string filters;
  for (size_t index = 0; index < accept_filters.size(); ++index) {
    const std::string filter = FilterJson(
        accept_filters[index].ToString(),
        index < accept_extensions.size() ? accept_extensions[index].ToString()
                                         : std::string(),
        index < accept_descriptions.size()
            ? accept_descriptions[index].ToString()
            : std::string());
    if (!filter.empty()) {
      filters += (filters.empty() ? "" : ",") + filter;
    }
  }
  const uint32_t id = ++file_dialog_serial_;
  const std::string default_path = default_file_path.ToString();
  const std::string payload =
      "{\"id\":" + std::to_string(id) + ",\"mode\":\"" + ModeName(mode) +
      "\",\"title\":" + JsString(title.ToString()) + ",\"defaultPath\":" +
      (default_path.empty() ? "null" : JsString(default_path)) +
      ",\"filters\":[" + filters + "]}";
  if (!SendMessage(kFileDialog, 0, 0, 0, 0, payload.data(),
                   static_cast<uint32_t>(payload.size()))) {
    return false;
  }
  file_dialogs_[id] = callback;
  return true;
}

void SabineOsrHandler::FinishFileDialog(const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const auto dialog = file_dialogs_.find(
      static_cast<uint32_t>(std::strtoul(parts[1].c_str(), nullptr, 10)));
  if (dialog == file_dialogs_.end()) {
    return;
  }
  CefRefPtr<CefFileDialogCallback> callback = dialog->second;
  file_dialogs_.erase(dialog);
  if (parts.size() < 3) {
    callback->Cancel();
    return;
  }
  std::vector<CefString> paths;
  for (const std::string& path : JsonStringArrayValue(parts[2], "paths")) {
    paths.push_back(path);
  }
  callback->Continue(paths);
}
