#ifndef SABINE_CEF_HOST_OSR_BROWSER_MEDIA_STATE_H_
#define SABINE_CEF_HOST_OSR_BROWSER_MEDIA_STATE_H_

#include <set>
#include <string>
#include <utility>

// The frames playing media and the browsers capturing a camera or
// microphone, which keep the window from freezing its pages.
struct MediaState {
  std::set<std::pair<int, std::string>> playing_frames;
  std::set<int> capturing_browsers;
  bool reported = false;

  bool Playing() const {
    return !playing_frames.empty() || !capturing_browsers.empty();
  }
  void Forget(int browser_id);
};

#endif
