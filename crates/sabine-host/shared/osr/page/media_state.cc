#include "osr/page/media_state.h"

#include <iterator>

#include "osr/transport/message_kinds.h"

void MediaState::SetFramePlaying(int browser_id,
                                 std::string frame_id,
                                 bool playing) {
  auto key = std::make_pair(browser_id, std::move(frame_id));
  if (playing) {
    playing_frames_.insert(std::move(key));
  } else {
    playing_frames_.erase(key);
  }
  Report();
}

void MediaState::SetCapturing(int browser_id, bool capturing) {
  if (capturing) {
    capturing_browsers_.insert(browser_id);
  } else {
    capturing_browsers_.erase(browser_id);
  }
  Report();
}

void MediaState::Forget(int browser_id) {
  capturing_browsers_.erase(browser_id);
  for (auto it = playing_frames_.begin(); it != playing_frames_.end();) {
    it = it->first == browser_id ? playing_frames_.erase(it) : std::next(it);
  }
}

void MediaState::Report() {
  const bool playing = Playing();
  if (playing == reported_) {
    return;
  }
  if (transport_.Send(kMediaPlaying, playing ? 1 : 0, 0, 0, 0, nullptr, 0)) {
    reported_ = playing;
  }
}
