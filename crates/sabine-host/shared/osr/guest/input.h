#ifndef SABINE_CEF_HOST_OSR_GUEST_INPUT_H_
#define SABINE_CEF_HOST_OSR_GUEST_INPUT_H_

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

bool IsPredominantlyHorizontalWheel(double delta_x, double delta_y);

#endif
