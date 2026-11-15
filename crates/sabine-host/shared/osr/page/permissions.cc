#include "osr/page/permissions.h"

#include <utility>

#include "common/json.h"
#include "include/internal/cef_types.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/transport/message_kinds.h"

namespace {

constexpr size_t kMaxPending = 32;

struct PermissionBits {
  uint32_t bits;
  const char* name;
};

constexpr PermissionBits kPromptPermissions[] = {
    {CEF_PERMISSION_TYPE_CAMERA_STREAM |
         CEF_PERMISSION_TYPE_CAMERA_PAN_TILT_ZOOM,
     "camera"},
    {CEF_PERMISSION_TYPE_MIC_STREAM, "microphone"},
    {CEF_PERMISSION_TYPE_CAPTURED_SURFACE_CONTROL, "screen"},
    {CEF_PERMISSION_TYPE_GEOLOCATION, "geolocation"},
    {CEF_PERMISSION_TYPE_NOTIFICATIONS, "notifications"},
    {CEF_PERMISSION_TYPE_MIDI_SYSEX, "midi"},
    {CEF_PERMISSION_TYPE_CLIPBOARD, "clipboard"},
    {CEF_PERMISSION_TYPE_POINTER_LOCK, "pointer-lock"},
    {CEF_PERMISSION_TYPE_KEYBOARD_LOCK, "keyboard-lock"},
    {CEF_PERMISSION_TYPE_LOCAL_FONTS, "local-fonts"},
    {CEF_PERMISSION_TYPE_WINDOW_MANAGEMENT, "window-management"},
    {CEF_PERMISSION_TYPE_IDLE_DETECTION, "idle-detection"},
    {CEF_PERMISSION_TYPE_STORAGE_ACCESS |
         CEF_PERMISSION_TYPE_TOP_LEVEL_STORAGE_ACCESS,
     "storage-access"},
    {CEF_PERMISSION_TYPE_PROTECTED_MEDIA_IDENTIFIER, "protected-media"},
    {CEF_PERMISSION_TYPE_FILE_SYSTEM_ACCESS, "file-system"},
    {CEF_PERMISSION_TYPE_SENSORS, "sensors"},
    {CEF_PERMISSION_TYPE_LOCAL_NETWORK_ACCESS_DEPRECATED |
         CEF_PERMISSION_TYPE_LOCAL_NETWORK |
         CEF_PERMISSION_TYPE_LOOPBACK_NETWORK,
     "local-network"},
    {CEF_PERMISSION_TYPE_MULTIPLE_DOWNLOADS, "downloads"},
};

constexpr PermissionBits kMediaPermissions[] = {
    {CEF_MEDIA_PERMISSION_DEVICE_VIDEO_CAPTURE, "camera"},
    {CEF_MEDIA_PERMISSION_DEVICE_AUDIO_CAPTURE, "microphone"},
    {CEF_MEDIA_PERMISSION_DESKTOP_VIDEO_CAPTURE |
         CEF_MEDIA_PERMISSION_DESKTOP_AUDIO_CAPTURE,
     "screen"},
};

template <size_t N>
std::vector<std::string> Names(const PermissionBits (&table)[N],
                               uint32_t requested) {
  std::vector<std::string> names;
  uint32_t known = 0;
  for (const PermissionBits& entry : table) {
    known |= entry.bits;
    if (requested & entry.bits) {
      names.push_back(entry.name);
    }
  }
  if (requested & ~known) {
    names.clear();
  }
  return names;
}

std::string AskJson(uint32_t id, const PermissionAsk& ask) {
  std::string permissions;
  for (const std::string& permission : ask.permissions) {
    permissions += (permissions.empty() ? "" : ",") + JsString(permission);
  }
  return "{\"id\":" + std::to_string(id) +
         ",\"origin\":" + JsString(ask.origin) +
         ",\"app\":" + (ask.app ? "true" : "false") +
         ",\"guest\":" + (ask.guest.empty() ? "null" : JsString(ask.guest)) +
         ",\"permissions\":[" + permissions + "]}";
}

}  // namespace

std::vector<std::string> PromptPermissions(uint32_t requested) {
  return Names(kPromptPermissions, requested);
}

std::vector<std::string> MediaPermissions(uint32_t requested) {
  return Names(kMediaPermissions, requested);
}

void PagePermissions::Ask(const PermissionAsk& ask, Answer answer) {
  CEF_REQUIRE_UI_THREAD();
  if (pending_.size() >= kMaxPending) {
    answer(false);
    return;
  }
  const uint32_t id = ++serial_;
  const std::string payload = AskJson(id, ask);
  if (!transport_.Send(kPermissionRequest, 0, 0, 0, 0, payload.data(),
                       static_cast<uint32_t>(payload.size()))) {
    answer(false);
    return;
  }
  pending_[id] = {ask.browser_id, ask.prompt_id, std::move(answer)};
}

void PagePermissions::Finish(uint32_t id, bool allowed) {
  CEF_REQUIRE_UI_THREAD();
  const auto request = pending_.find(id);
  if (request == pending_.end()) {
    return;
  }
  Answer answer = std::move(request->second.answer);
  pending_.erase(request);
  answer(allowed);
}

void PagePermissions::Dismiss(uint64_t prompt_id) {
  CEF_REQUIRE_UI_THREAD();
  for (auto request = pending_.begin(); request != pending_.end(); ++request) {
    if (request->second.prompt_id == prompt_id) {
      Withdraw(request);
      return;
    }
  }
}

void PagePermissions::Forget(int browser_id) {
  CEF_REQUIRE_UI_THREAD();
  for (auto request = pending_.begin(); request != pending_.end();) {
    auto next = std::next(request);
    if (request->second.browser_id == browser_id) {
      Withdraw(request);
    }
    request = next;
  }
}

void PagePermissions::Withdraw(std::map<uint32_t, Pending>::iterator request) {
  transport_.Send(kPermissionWithdrawn, request->first, 0, 0, 0, nullptr, 0);
  pending_.erase(request);
}
