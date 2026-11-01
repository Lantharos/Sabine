#include "osr/browser/media_state.h"

#include <iterator>
#include <string>
#include <utility>

#include "include/cef_values.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

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
    media_.playing_frames.insert(std::move(key));
  } else {
    media_.playing_frames.erase(key);
  }
  ReportMediaPlaying();
}

void SabineOsrHandler::OnMediaAccessChange(CefRefPtr<CefBrowser> browser,
                                           bool has_video_access,
                                           bool has_audio_access) {
  CEF_REQUIRE_UI_THREAD();
  if (has_video_access || has_audio_access) {
    media_.capturing_browsers.insert(browser->GetIdentifier());
  } else {
    media_.capturing_browsers.erase(browser->GetIdentifier());
  }
  ReportMediaPlaying();
}

void MediaState::Forget(int browser_id) {
  capturing_browsers.erase(browser_id);
  for (auto it = playing_frames.begin(); it != playing_frames.end();) {
    it = it->first == browser_id ? playing_frames.erase(it) : std::next(it);
  }
}

void SabineOsrHandler::ReportMediaPlaying() {
  const bool playing = media_.Playing();
  if (playing == media_.reported) {
    return;
  }
  if (SendMessage(kMediaPlaying, playing ? 1 : 0, 0, 0, 0, nullptr, 0)) {
    media_.reported = playing;
  }
}
