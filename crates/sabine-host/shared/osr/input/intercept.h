#ifndef SABINE_CEF_HOST_OSR_INPUT_INTERCEPT_H_
#define SABINE_CEF_HOST_OSR_INPUT_INTERCEPT_H_

#include <cstdint>
#include <string>
#include <vector>

constexpr uint32_t kGuestModShift = 1u << 1;
constexpr uint32_t kGuestModControl = 1u << 2;
constexpr uint32_t kGuestModAlt = 1u << 3;
constexpr uint32_t kGuestModCommand = 1u << 7;
constexpr uint32_t kGuestModRepeat = 1u << 13;
constexpr uint32_t kGuestModMask =
    kGuestModShift | kGuestModControl | kGuestModAlt | kGuestModCommand;

const std::string* MatchInterceptedShortcut(
    const std::vector<std::string>& shortcuts,
    const std::string& key,
    uint32_t modifiers);

// The key and modifiers of a menu item's accelerator, which the window sends
// as `Control+Alt+Shift+Meta+Key` with the modifiers it holds.
struct KeyChord {
  uint32_t modifiers;
  int key_code;
  std::string accelerator;
};

std::vector<KeyChord> ParseKeyChords(
    const std::vector<std::string>& accelerators);
const KeyChord* MatchKeyChord(const std::vector<KeyChord>& chords,
                              uint32_t modifiers,
                              int key_code);

bool IsPredominantlyHorizontalWheel(double delta_x, double delta_y);

#endif
