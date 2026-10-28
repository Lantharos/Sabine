#ifndef SABINE_DISPLAY_PRIVATE_DISPLAY_H_
#define SABINE_DISPLAY_PRIVATE_DISPLAY_H_

namespace sabine_display {

// Chromium renders off screen, yet its Wayland platform still connects to the
// desktop's compositor, and ends the browser when that connection breaks, as
// when the compositor crashes and starts again. The browser gets a display of
// its own instead, inside this process, which offers what Chromium binds and
// the buffer formats the compositor imports. Pages then outlive the
// compositor while their windows reconnect to it.
void UsePrivateDisplay();

void ClosePrivateDisplay();

}  // namespace sabine_display

#endif
