#ifndef SABINE_DISPLAY_PRIVATE_DISPLAY_H_
#define SABINE_DISPLAY_PRIVATE_DISPLAY_H_

#include <sys/types.h>

#include <optional>

namespace sabine_display {

// Chromium renders off screen, yet its Wayland platform still connects to the
// desktop's compositor, and ends the browser when that connection breaks, as
// when the compositor crashes and starts again. The browser gets a display of
// its own instead, inside this process, which offers what Chromium binds and
// the buffer formats the compositor imports. Pages then outlive the
// compositor while their windows reconnect to it.
void UsePrivateDisplay();

void ClosePrivateDisplay();

// The device the desktop's compositor composites on, which Chromium renders
// its frames on too, or nothing without a private display.
std::optional<dev_t> CompositorMainDevice();

}  // namespace sabine_display

#endif
