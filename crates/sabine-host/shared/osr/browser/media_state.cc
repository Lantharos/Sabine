#include "osr/handler.h"

#include <iterator>
#include <string>
#include <utility>

#include "include/cef_values.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

namespace {

void SetLifecycleState(CefRefPtr<CefBrowser> browser, const char* state) {
  CefRefPtr<CefDictionaryValue> params = CefDictionaryValue::Create();
  params->SetString("state", state);
  browser->GetHost()->ExecuteDevToolsMethod(0, "Page.setWebLifecycleState",
                                            params);
}

void PurgeMemory(CefRefPtr<CefBrowser> browser) {
  CefRefPtr<CefDictionaryValue> params = CefDictionaryValue::Create();
  params->SetString("level", "critical");
  browser->GetHost()->ExecuteDevToolsMethod(
      0, "Memory.simulatePressureNotification", params);
}

}  // namespace

// A frozen page runs no JavaScript at all, and its caches, decoded images and
// GPU resources are released, while everything it holds stays in place for an
// instant thaw.
void SabineOsrHandler::SetPagesFrozen(bool frozen) {
  CEF_REQUIRE_UI_THREAD();
  if (frozen == frozen_) {
    return;
  }
  frozen_ = frozen;
  for (auto& browser : browsers_) {
    SetLifecycleState(browser, frozen ? "frozen" : "active");
    if (frozen) {
      PurgeMemory(browser);
    }
  }
}

void SabineOsrHandler::UpdateFrameMediaState(CefRefPtr<CefBrowser> browser,
                                             CefRefPtr<CefFrame> frame,
                                             bool playing) {
  CEF_REQUIRE_UI_THREAD();
  auto key = std::make_pair(browser->GetIdentifier(),
                            frame->GetIdentifier().ToString());
  if (playing) {
    playing_frames_.insert(std::move(key));
  } else {
    playing_frames_.erase(key);
  }
  ReportMediaPlaying();
}

void SabineOsrHandler::OnMediaAccessChange(CefRefPtr<CefBrowser> browser,
                                           bool has_video_access,
                                           bool has_audio_access) {
  CEF_REQUIRE_UI_THREAD();
  if (has_video_access || has_audio_access) {
    capturing_browsers_.insert(browser->GetIdentifier());
  } else {
    capturing_browsers_.erase(browser->GetIdentifier());
  }
  ReportMediaPlaying();
}

void SabineOsrHandler::ForgetMediaState(CefRefPtr<CefBrowser> browser) {
  const int id = browser->GetIdentifier();
  capturing_browsers_.erase(id);
  for (auto it = playing_frames_.begin(); it != playing_frames_.end();) {
    it = it->first == id ? playing_frames_.erase(it) : std::next(it);
  }
  ReportMediaPlaying();
}

void SabineOsrHandler::ReportMediaPlaying() {
  const bool playing = !playing_frames_.empty() || !capturing_browsers_.empty();
  if (playing == media_playing_reported_) {
    return;
  }
  if (SendMessage(kMediaPlaying, playing ? 1 : 0, 0, 0, 0, nullptr, 0)) {
    media_playing_reported_ = playing;
  }
}
