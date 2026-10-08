#include "osr/handler.h"

#include <cstdlib>
#include <string>
#include <utility>
#include <vector>

#include "common/bridge_policy.h"
#include "common/json.h"
#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

namespace {

constexpr int64_t kDragOverRepeatMs = 350;

// Chromium's stand-in for links a page may not hand to other apps, such as
// the file URIs a page puts in its own drags.
constexpr char kBlockedUrl[] = "about:blank#blocked";

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
  std::string url = data->GetLinkURL().ToString();
  if (url == kBlockedUrl) {
    url.clear();
  }
  return "{\"text\":" + JsString(data->GetFragmentText().ToString()) +
         ",\"html\":" + JsString(data->GetFragmentHtml().ToString()) +
         ",\"url\":" + JsString(url) +
         ",\"urlTitle\":" + JsString(data->GetLinkTitle().ToString()) +
         ",\"files\":[" + files +
         "],\"operations\":" + std::to_string(operations) + "}";
}

CefRefPtr<CefDragData> DragDataFromJson(const std::string& json) {
  CefRefPtr<CefDragData> data = CefDragData::Create();
  const std::string text = JsonStringValue(json, "text");
  const std::string html = JsonStringValue(json, "html");
  const std::string url = JsonStringValue(json, "url");
  if (!text.empty()) {
    data->SetFragmentText(text);
  }
  if (!html.empty()) {
    data->SetFragmentHtml(html);
  }
  if (!url.empty()) {
    data->SetLinkURL(url);
  }
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
  if (command.rfind("drag_", 0) != 0) {
    return false;
  }
  if (command == "drag_enter" && parts.size() >= 6) {
    held_drag_controls_.clear();
    awaiting_dropped_paths_ = SendDroppedPaths(parts[5]);
  }
  if (awaiting_dropped_paths_) {
    held_drag_controls_.push_back(parts);
  } else {
    RunDragControl(parts);
  }
  return true;
}

void SabineOsrHandler::RunDragControl(const std::vector<std::string>& parts) {
  const std::string& command = parts[0];
  CefRefPtr<CefBrowserHost> host = browser_->GetHost();
  if (command == "drag_enter" && parts.size() >= 6) {
    drag_operation_ = DRAG_OPERATION_NONE;
    drag_target_operations_ = Operations(parts[4]);
    host->DragTargetDragEnter(DragDataFromJson(parts[5]), DragEvent(parts),
                              drag_target_operations_);
    KeepDraggingOver(DragEvent(parts));
  } else if (command == "drag_enter_source" && parts.size() >= 4) {
    if (!drag_source_data_) {
      return;
    }
    drag_operation_ = DRAG_OPERATION_NONE;
    drag_target_operations_ = drag_source_operations_;
    host->DragTargetDragEnter(drag_source_data_->Clone(), DragEvent(parts),
                              drag_target_operations_);
    KeepDraggingOver(DragEvent(parts));
  } else if (command == "drag_over" && parts.size() >= 5) {
    drag_target_operations_ = Operations(parts[4]);
    host->DragTargetDragOver(DragEvent(parts), drag_target_operations_);
    KeepDraggingOver(DragEvent(parts));
  } else if (command == "drag_leave") {
    StopDraggingOver();
    host->DragTargetDragLeave();
  } else if (command == "drag_drop" && parts.size() >= 4) {
    const CefMouseEvent event = DragEvent(parts);
    StopDraggingOver();
    host->DragTargetDrop(event);
    if (drag_source_browser_ && drag_source_data_) {
      EndDragSource(event.x, event.y, drag_operation_);
    }
  } else if (command == "drag_source_ended" && parts.size() >= 4) {
    EndDragSource(std::atoi(parts[1].c_str()), std::atoi(parts[2].c_str()),
                  Operations(parts[3]));
  }
}

// Browsers repeat dragover while the pointer rests over a page, which is how
// the element under it gets to accept a drop it reached with its last move.
void SabineOsrHandler::KeepDraggingOver(const CefMouseEvent& event) {
  drag_target_event_ = event;
  CefPostDelayedTask(
      TID_UI,
      base::BindOnce(&SabineOsrHandler::RepeatDragOver,
                     CefRefPtr<SabineOsrHandler>(this), ++drag_over_serial_),
      kDragOverRepeatMs);
}

void SabineOsrHandler::StopDraggingOver() {
  ++drag_over_serial_;
}

void SabineOsrHandler::RepeatDragOver(uint64_t serial) {
  CEF_REQUIRE_UI_THREAD();
  if (serial != drag_over_serial_ || !browser_) {
    return;
  }
  browser_->GetHost()->DragTargetDragOver(drag_target_event_,
                                          drag_target_operations_);
  KeepDraggingOver(drag_target_event_);
}

// The app's own page learns the paths of files dropped on it before it hears
// about the drag, so it can read them from its drag events. Drag events wait
// until the page has them.
bool SabineOsrHandler::SendDroppedPaths(const std::string& content) {
  const std::vector<std::string> paths = JsonStringArrayValue(content, "files");
  CefRefPtr<CefFrame> frame = browser_->GetMainFrame();
  if (paths.empty() || !frame ||
      !sabine_bridge::AllowsDocument(BridgePolicyFor(browser_),
                                     frame->GetURL())) {
    return false;
  }
  CefRefPtr<CefProcessMessage> message =
      CefProcessMessage::Create("sabine.drop.paths");
  CefRefPtr<CefListValue> arguments = message->GetArgumentList();
  arguments->SetString(0, std::to_string(++dropped_paths_serial_));
  for (size_t index = 0; index < paths.size(); ++index) {
    arguments->SetString(index + 1, paths[index]);
  }
  frame->SendProcessMessage(PID_RENDERER, message);
  return true;
}

void SabineOsrHandler::DroppedPathsDelivered(const std::string& serial) {
  CEF_REQUIRE_UI_THREAD();
  if (!awaiting_dropped_paths_ ||
      serial != std::to_string(dropped_paths_serial_)) {
    return;
  }
  awaiting_dropped_paths_ = false;
  const std::vector<std::vector<std::string>> held =
      std::move(held_drag_controls_);
  held_drag_controls_.clear();
  for (const std::vector<std::string>& parts : held) {
    RunDragControl(parts);
  }
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
