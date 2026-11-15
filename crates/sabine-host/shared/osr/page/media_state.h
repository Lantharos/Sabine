#ifndef SABINE_CEF_HOST_OSR_PAGE_MEDIA_STATE_H_
#define SABINE_CEF_HOST_OSR_PAGE_MEDIA_STATE_H_

#include <set>
#include <string>
#include <utility>

#include "osr/transport/transport.h"

// The frames playing media and the browsers capturing a camera or
// microphone, which keep the window from freezing its pages.
class MediaState {
 public:
  explicit MediaState(sabine_osr::Transport& transport)
      : transport_(transport) {}

  void SetFramePlaying(int browser_id, std::string frame_id, bool playing);
  void SetCapturing(int browser_id, bool capturing);
  void ForgetFrame(int browser_id, const std::string& frame_id) {
    playing_frames_.erase({browser_id, frame_id});
  }
  void Forget(int browser_id);
  // Tells the window whether any page plays or captures, when that changed.
  void Report();

 private:
  bool Playing() const {
    return !playing_frames_.empty() || !capturing_browsers_.empty();
  }

  sabine_osr::Transport& transport_;
  std::set<std::pair<int, std::string>> playing_frames_;
  std::set<int> capturing_browsers_;
  bool reported_ = false;
};

#endif
