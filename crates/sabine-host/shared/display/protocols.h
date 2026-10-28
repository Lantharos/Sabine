#ifndef SABINE_DISPLAY_PROTOCOLS_H_
#define SABINE_DISPLAY_PROTOCOLS_H_

#include "display/wayland.h"

// xdg-shell version 1 and linux-dmabuf version 4, as libwayland describes
// them. Chromium binds both before it renders, even off screen.
namespace sabine_display {

extern const WlInterface kXdgWmBase;
extern const WlInterface kLinuxDmabuf;
extern const WlInterface kLinuxBufferParams;
extern const WlInterface kLinuxDmabufFeedback;

namespace dmabuf {
constexpr uint32_t kCreateParams = 1;
constexpr uint32_t kGetDefaultFeedback = 2;
constexpr uint32_t kGetSurfaceFeedback = 3;
constexpr uint32_t kFormatEvent = 0;
constexpr uint32_t kModifierEvent = 1;
}  // namespace dmabuf

namespace params {
constexpr uint32_t kCreate = 2;
constexpr uint32_t kCreateImmed = 3;
constexpr uint32_t kFailedEvent = 1;
}  // namespace params

namespace feedback {
constexpr uint32_t kDone = 0;
constexpr uint32_t kFormatTable = 1;
constexpr uint32_t kMainDevice = 2;
constexpr uint32_t kTrancheDone = 3;
constexpr uint32_t kTrancheTargetDevice = 4;
constexpr uint32_t kTrancheFormats = 5;
constexpr uint32_t kTrancheFlags = 6;
}  // namespace feedback

}  // namespace sabine_display

#endif
