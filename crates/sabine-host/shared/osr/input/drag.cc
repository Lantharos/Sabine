#include "osr/handler.h"

#include <algorithm>
#include <cctype>
#include <cstdint>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#include "guest/input.h"
#include "guest/manager.h"
#include "include/cef_app.h"
#include "include/cef_browser.h"
#include "include/cef_parser.h"
#include "include/cef_request_context_handler.h"
#include "include/cef_task.h"
#include "include/internal/cef_types.h"
#include "include/wrapper/cef_helpers.h"
#include "common/json.h"
#include "sabine_bridge_js.h"
#include "osr/utilities.h"

using namespace sabine_osr;

namespace {

std::string FileUriToPath(const std::string& value) {
  constexpr std::string_view kScheme = "file://";
  if (value.rfind(kScheme, 0) != 0) {
    return value;
  }
  const size_t path_start = value.find('/', kScheme.size());
  if (path_start == std::string::npos) {
    return std::string();
  }
  const std::string_view authority(value.data() + kScheme.size(),
                                   path_start - kScheme.size());
  if (!authority.empty() && authority != "localhost") {
    return std::string();
  }
  const size_t path_end = value.find_first_of("?#", path_start);
  std::string path = DecodeControlComponent(value.substr(
      path_start, path_end == std::string::npos ? std::string::npos
                                                : path_end - path_start));
#if defined(OS_WIN)
  if (path.size() >= 3 && path[0] == '/' &&
      std::isalpha(static_cast<unsigned char>(path[1])) && path[2] == ':') {
    path.erase(0, 1);
  }
  std::replace(path.begin(), path.end(), '/', '\\');
#endif
  return path;
}

std::string BuildFileDragPayload(const std::vector<std::string>& paths) {
  std::string output = "{\"paths\":[";
  bool first = true;
  for (const auto& path : paths) {
    if (!first) {
      output += ",";
    }
    first = false;
    output += '"';
    output += JsonEscape(path);
    output += '"';
  }
  output += "]}";
  return output;
}

cef_drag_operations_mask_t DragOperationFromName(const std::string& operation) {
  if (operation == "copy") {
    return DRAG_OPERATION_COPY;
  }
  if (operation == "move") {
    return DRAG_OPERATION_MOVE;
  }
  if (operation == "link") {
    return DRAG_OPERATION_LINK;
  }
  return DRAG_OPERATION_NONE;
}

}  // namespace

bool SabineOsrHandler::StartDragging(CefRefPtr<CefBrowser> browser,
                                     CefRefPtr<CefDragData> drag_data,
                                     cef_drag_operations_mask_t allowed_ops,
                                     int x,
                                     int y) {
  CEF_REQUIRE_UI_THREAD();
  if (!browser || !drag_data || socket_fd_ < 0) {
    return false;
  }

  std::vector<std::string> paths;

  if (drag_data->IsFile()) {
    std::vector<CefString> file_paths;
    if (drag_data->GetFilePaths(file_paths) && !file_paths.empty()) {
      for (const auto& file_path : file_paths) {
        paths.push_back(FileUriToPath(file_path.ToString()));
      }
    }
    if (paths.empty()) {
      const std::string file_name = drag_data->GetFileName().ToString();
      if (!file_name.empty()) {
        paths.push_back(FileUriToPath(file_name));
      }
    }
  }

  if (paths.empty()) {
    const std::string fragment_text = drag_data->GetFragmentText().ToString();
    const std::string link_url = drag_data->GetLinkURL().ToString();
    std::stringstream stream;
    if (!fragment_text.empty()) {
      stream << fragment_text;
    } else if (!link_url.empty()) {
      stream << link_url;
    }
    std::string line;
    while (std::getline(stream, line)) {
      std::string trimmed = line;
      while (!trimmed.empty() &&
             (trimmed.back() == '\r' || trimmed.back() == '\n')) {
        trimmed.pop_back();
      }
      if (trimmed.empty())
        continue;
      paths.push_back(FileUriToPath(trimmed));
    }
  }

  if (paths.empty()) {
    return false;
  }

  const std::string payload = BuildFileDragPayload(paths);
  if (!SendMessage(kFileDragRequested, 0, 0, x, y, payload.data(),
                   static_cast<uint32_t>(payload.size()))) {
    return false;
  }

  // The Rust host owns the system drag via winit. Keep CEF's drag source
  // alive until the host reports completion through file_drag_ended.
  drag_source_browser_ = browser;
  (void)allowed_ops;
  return true;
}

void SabineOsrHandler::UpdateDragCursor(CefRefPtr<CefBrowser> browser,
                                        DragOperation operation) {
  // No-op: cursor changes are driven by the host's window manager.
  (void)browser;
  (void)operation;
}

void SabineOsrHandler::FinishNativeFileDrag(int x,
                                            int y,
                                            const std::string& operation) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser = drag_source_browser_;
  drag_source_browser_ = nullptr;
  if (!browser) {
    browser = browser_;
  }
  if (!browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = browser->GetHost();
  if (!host) {
    return;
  }
  host->DragSourceEndedAt(x, y, DragOperationFromName(operation));
  host->DragSourceSystemDragEnded();
}
