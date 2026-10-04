#include "osr/handler.h"

#include <cstdlib>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

namespace {

std::string DragPayload(CefRefPtr<CefDragData> data,
                        cef_drag_operations_mask_t operations) {
  std::string files;
  std::vector<CefString> paths;
  if (data->IsFile() && data->GetFilePaths(paths)) {
    for (const CefString& path : paths) {
      files +=
          (files.empty() ? "\"" : ",\"") + JsonEscape(path.ToString()) + "\"";
    }
  }
  return "{\"text\":" + JsString(data->GetFragmentText().ToString()) +
         ",\"html\":" + JsString(data->GetFragmentHtml().ToString()) +
         ",\"url\":" + JsString(data->GetLinkURL().ToString()) +
         ",\"urlTitle\":" + JsString(data->GetLinkTitle().ToString()) +
         ",\"files\":[" + files +
         "],\"operations\":" + std::to_string(operations) + "}";
}

CefRefPtr<CefDragData> DragDataFromJson(const std::string& json) {
  CefRefPtr<CefDragData> data = CefDragData::Create();
  data->SetFragmentText(JsonStringValue(json, "text"));
  data->SetFragmentHtml(JsonStringValue(json, "html"));
  data->SetLinkURL(JsonStringValue(json, "url"));
  for (const std::string& path : JsonStringArrayValue(json, "files")) {
    data->AddFile(path, CefString());
  }
  return data;
}

CefMouseEvent DragEvent(const std::vector<std::string>& parts) {
  CefMouseEvent event;
  event.x = std::atoi(parts[1].c_str());
  event.y = std::atoi(parts[2].c_str());
  event.modifiers = std::strtoul(parts[3].c_str(), nullptr, 10);
  return event;
}

cef_drag_operations_mask_t Operations(const std::string& value) {
  return static_cast<cef_drag_operations_mask_t>(
      std::strtoul(value.c_str(), nullptr, 10));
}

}  // namespace

bool SabineOsrHandler::StartDragging(CefRefPtr<CefBrowser> browser,
                                     CefRefPtr<CefDragData> drag_data,
                                     cef_drag_operations_mask_t allowed_ops,
                                     int x,
                                     int y) {
  CEF_REQUIRE_UI_THREAD();
  const std::string payload = DragPayload(drag_data, allowed_ops);
  if (!SendMessage(kDragStarted, 0, 0, 0, 0, payload.data(),
                   static_cast<uint32_t>(payload.size()))) {
    return false;
  }
  drag_source_browser_ = browser;
  drag_source_data_ = drag_data->Clone();
  drag_source_operations_ = allowed_ops;
  return true;
}

void SabineOsrHandler::UpdateDragCursor(CefRefPtr<CefBrowser> browser,
                                        DragOperation operation) {
  CEF_REQUIRE_UI_THREAD();
  if (operation == drag_operation_) {
    return;
  }
  drag_operation_ = operation;
  SendMessage(kDragOperation, static_cast<uint32_t>(operation), 0, 0, 0,
              nullptr, 0);
}

bool SabineOsrHandler::TryHandleDragControl(
    const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  const std::string& command = parts[0];
  CefRefPtr<CefBrowserHost> host = browser_->GetHost();
  if (command == "drag_enter" && parts.size() >= 6) {
    drag_operation_ = DRAG_OPERATION_NONE;
    host->DragTargetDragEnter(DragDataFromJson(parts[5]), DragEvent(parts),
                              Operations(parts[4]));
  } else if (command == "drag_enter_source" && parts.size() >= 4) {
    if (!drag_source_data_) {
      return true;
    }
    drag_operation_ = DRAG_OPERATION_NONE;
    host->DragTargetDragEnter(drag_source_data_->Clone(), DragEvent(parts),
                              drag_source_operations_);
  } else if (command == "drag_over" && parts.size() >= 5) {
    host->DragTargetDragOver(DragEvent(parts), Operations(parts[4]));
  } else if (command == "drag_leave") {
    host->DragTargetDragLeave();
  } else if (command == "drag_drop" && parts.size() >= 4) {
    const CefMouseEvent event = DragEvent(parts);
    host->DragTargetDrop(event);
    if (drag_source_browser_ && drag_source_data_) {
      EndDragSource(event.x, event.y, drag_operation_);
    }
  } else if (command == "drag_source_ended" && parts.size() >= 4) {
    EndDragSource(std::atoi(parts[1].c_str()), std::atoi(parts[2].c_str()),
                  Operations(parts[3]));
  } else {
    return false;
  }
  return true;
}

void SabineOsrHandler::EndDragSource(int x,
                                     int y,
                                     cef_drag_operations_mask_t operation) {
  CefRefPtr<CefBrowser> browser = drag_source_browser_;
  drag_source_browser_ = nullptr;
  drag_source_data_ = nullptr;
  if (!browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = browser->GetHost();
  host->DragSourceEndedAt(x, y, operation);
  host->DragSourceSystemDragEnded();
}
