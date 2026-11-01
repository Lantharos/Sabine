#ifndef SABINE_CEF_HOST_OSR_WINDOW_STATE_H_
#define SABINE_CEF_HOST_OSR_WINDOW_STATE_H_

// How the desktop shows the window, as the window last reported it.
struct WindowState {
  bool shown = true;
  bool visible = true;
  bool occluded = false;
  bool suspended = false;
  bool maximized = false;
  bool fullscreen = false;
  bool tiled = false;

  bool Floating() const { return !(maximized || fullscreen || tiled); }
};

#endif
