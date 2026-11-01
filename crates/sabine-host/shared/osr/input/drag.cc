#include "osr/input/drag.h"

#include <cstdlib>
#include <string>
#include <utility>
#include <vector>

#include "common/bridge_policy.h"
#include "common/json.h"
#include "osr/transport/wire.h"
#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

using namespace sabine_osr;

namespace {

constexpr int64_t kDragOverRepeatMs = 350;

// Chromium's stand-in for links a page may not hand to other apps, such as
// the file URIs a page puts in its own drags.
constexpr char kBlockedUrl[] = "about:blank#blocked";

struct DragImage {
  std::vector<char> pixels;
  int width = 0;
  int height = 0;
  CefPoint hotspot;
  float scale = 1.0f;
};

// The picture Chromium drew of what is dragged, at the page's scale, with
// the pointer's place on it in the same pixels.
DragImage DragImageOf(CefRefPtr<CefDragData> data, float scale) {
  DragImage image;
  CefRefPtr<CefImage> picture = data->HasImage() ? data->GetImage() : nullptr;
  if (!picture || picture->IsEmpty() || picture->GetWidth() == 0) {
    return image;
  }
  CefRefPtr<CefBinaryValue> bitmap = picture->GetAsBitmap(
      scale, CEF_COLOR_TYPE_RGBA_8888, CEF_ALPHA_TYPE_POSTMULTIPLIED,
      image.width, image.height);
  if (!bitmap || image.width <= 0 || image.height <= 0) {
    return DragImage();
  }
  image.pixels.resize(bitmap->GetSize());
  bitmap->GetData(image.pixels.data(), image.pixels.size(), 0);
  image.scale =
      static_cast<float>(image.width) / static_cast<float>(picture->GetWidth());
  const CefPoint hotspot = data->GetImageHotspot();
  image.hotspot = CefPoint(static_cast<int>(hotspot.x * image.scale),
                           static_cast<int>(hotspot.y * image.scale));
  return image;
}

std::string DragPayload(CefRefPtr<CefDragData> data,
                        cef_drag_operations_mask_t operations,
                        float image_scale) {
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
         "],\"operations\":" + std::to_string(operations) +
         ",\"imageScale\":" + std::to_string(image_scale) + "}";
}

CefRefPtr<CefDragData> DragDataFromJson(const JsonObject& content) {
  CefRefPtr<CefDragData> data = CefDragData::Create();
  const std::string text = content.String("text");
  const std::string html = content.String("html");
  const std::string url = content.String("url");
  if (!text.empty()) {
    data->SetFragmentText(text);
  }
  if (!html.empty()) {
    data->SetFragmentHtml(html);
  }
  if (!url.empty()) {
    data->SetLinkURL(url);
  }
  for (const std::string& path : content.Strings("files")) {
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
  const DragImage image = DragImageOf(drag_data, scale_);
  const std::string content = DragPayload(drag_data, allowed_ops, image.scale);
  std::vector<char> payload(4);
  PutU32(&payload, 0, static_cast<uint32_t>(content.size()));
  payload.insert(payload.end(), content.begin(), content.end());
  payload.insert(payload.end(), image.pixels.begin(), image.pixels.end());
  if (!SendMessage(kDragStarted, static_cast<uint32_t>(image.width),
                   static_cast<uint32_t>(image.height), image.hotspot.x,
                   image.hotspot.y, payload.data(),
                   static_cast<uint32_t>(payload.size()))) {
    return false;
  }
  drag_.source_browser = browser;
  drag_.source_data = drag_data->Clone();
  drag_.source_operations = allowed_ops;
  return true;
}

void SabineOsrHandler::UpdateDragCursor(CefRefPtr<CefBrowser> browser,
                                        DragOperation operation) {
  CEF_REQUIRE_UI_THREAD();
  if (operation == drag_.operation) {
    return;
  }
  drag_.operation = operation;
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
    drag_.held_controls.clear();
    drag_.awaiting_dropped_paths = SendDroppedPaths(parts[5]);
  }
  if (drag_.awaiting_dropped_paths) {
    drag_.held_controls.push_back(parts);
  } else {
    RunDragControl(parts);
  }
  return true;
}

void SabineOsrHandler::RunDragControl(const std::vector<std::string>& parts) {
  const std::string& command = parts[0];
  CefRefPtr<CefBrowserHost> host = browser_->GetHost();
  if (command == "drag_enter" && parts.size() >= 6) {
    drag_.operation = DRAG_OPERATION_NONE;
    drag_.target_operations = Operations(parts[4]);
    host->DragTargetDragEnter(DragDataFromJson(JsonObject::Parse(parts[5])),
                              DragEvent(parts), drag_.target_operations);
    KeepDraggingOver(DragEvent(parts));
  } else if (command == "drag_enter_source" && parts.size() >= 4) {
    if (!drag_.source_data) {
      return;
    }
    drag_.operation = DRAG_OPERATION_NONE;
    drag_.target_operations = drag_.source_operations;
    host->DragTargetDragEnter(drag_.source_data->Clone(), DragEvent(parts),
                              drag_.target_operations);
    KeepDraggingOver(DragEvent(parts));
  } else if (command == "drag_over" && parts.size() >= 5) {
    drag_.target_operations = Operations(parts[4]);
    host->DragTargetDragOver(DragEvent(parts), drag_.target_operations);
    KeepDraggingOver(DragEvent(parts));
  } else if (command == "drag_leave") {
    StopDraggingOver();
    host->DragTargetDragLeave();
  } else if (command == "drag_drop" && parts.size() >= 4) {
    const CefMouseEvent event = DragEvent(parts);
    StopDraggingOver();
    host->DragTargetDrop(event);
    if (drag_.source_browser && drag_.source_data) {
      EndDragSource(event.x, event.y, drag_.operation);
    }
  } else if (command == "drag_source_ended" && parts.size() >= 4) {
    EndDragSource(std::atoi(parts[1].c_str()), std::atoi(parts[2].c_str()),
                  Operations(parts[3]));
  }
}

// Browsers repeat dragover while the pointer rests over a page, which is how
// the element under it gets to accept a drop it reached with its last move.
void SabineOsrHandler::KeepDraggingOver(const CefMouseEvent& event) {
  drag_.target_event = event;
  CefPostDelayedTask(
      TID_UI,
      base::BindOnce(&SabineOsrHandler::RepeatDragOver,
                     CefRefPtr<SabineOsrHandler>(this), ++drag_.over_serial),
      kDragOverRepeatMs);
}

void SabineOsrHandler::StopDraggingOver() {
  ++drag_.over_serial;
}

void SabineOsrHandler::RepeatDragOver(uint64_t serial) {
  CEF_REQUIRE_UI_THREAD();
  if (serial != drag_.over_serial || !browser_) {
    return;
  }
  browser_->GetHost()->DragTargetDragOver(drag_.target_event,
                                          drag_.target_operations);
  KeepDraggingOver(drag_.target_event);
}

// The app's own page learns the paths of files dropped on it before it hears
// about the drag, so it can read them from its drag events. Drag events wait
// until the page has them.
bool SabineOsrHandler::SendDroppedPaths(const std::string& content) {
  const std::vector<std::string> paths =
      JsonObject::Parse(content).Strings("files");
  CefRefPtr<CefFrame> frame = browser_->GetMainFrame();
  if (paths.empty() || !frame ||
      !sabine_bridge::AllowsDocument(BridgePolicyFor(browser_),
                                     frame->GetURL())) {
    return false;
  }
  CefRefPtr<CefProcessMessage> message =
      CefProcessMessage::Create("sabine.drop.paths");
  CefRefPtr<CefListValue> arguments = message->GetArgumentList();
  arguments->SetString(0, std::to_string(++drag_.dropped_paths_serial));
  for (size_t index = 0; index < paths.size(); ++index) {
    arguments->SetString(index + 1, paths[index]);
  }
  frame->SendProcessMessage(PID_RENDERER, message);
  return true;
}

void SabineOsrHandler::DroppedPathsDelivered(const std::string& serial) {
  CEF_REQUIRE_UI_THREAD();
  if (!drag_.awaiting_dropped_paths ||
      serial != std::to_string(drag_.dropped_paths_serial)) {
    return;
  }
  drag_.awaiting_dropped_paths = false;
  const std::vector<std::vector<std::string>> held =
      std::move(drag_.held_controls);
  drag_.held_controls.clear();
  for (const std::vector<std::string>& parts : held) {
    RunDragControl(parts);
  }
}

void SabineOsrHandler::EndDragSource(int x,
                                     int y,
                                     cef_drag_operations_mask_t operation) {
  CefRefPtr<CefBrowser> browser = drag_.source_browser;
  drag_.source_browser = nullptr;
  drag_.source_data = nullptr;
  if (!browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = browser->GetHost();
  host->DragSourceEndedAt(x, y, operation);
  host->DragSourceSystemDragEnded();
}
