#ifndef SABINE_CEF_HOST_OSR_GUEST_EVENTS_H_
#define SABINE_CEF_HOST_OSR_GUEST_EVENTS_H_

#include <cstdint>
#include <string>
#include <vector>

#include "osr/guest/registry.h"

std::string GuestInfoJson(const GuestView& guest);
std::string GuestListJson(const std::vector<const GuestView*>& guests);
std::string GuestIdJson(const std::string& id);
std::string GuestNavigatedJson(const GuestView& guest);
std::string GuestNewWindowJson(const std::string& id,
                               const std::string& url,
                               int disposition);
std::string GuestShortcutJson(const std::string& id,
                              const std::string& accelerator,
                              const std::string& key,
                              bool repeat,
                              uint32_t modifiers);
std::string GuestWheelJson(const std::string& id,
                           double delta_x,
                           double delta_y,
                           uint32_t modifiers);
std::string GuestFaviconJson(const std::string& id,
                             const std::vector<std::string>& favicons);

#endif
