#ifndef SABINE_DISPLAY_OFFER_H_
#define SABINE_DISPLAY_OFFER_H_

#include <cstdint>
#include <optional>
#include <vector>

namespace sabine_display {

struct Tranche {
  std::vector<uint8_t> target_device;
  std::vector<uint8_t> formats;
  uint32_t flags = 0;
};

struct FormatModifier {
  uint32_t format;
  uint32_t modifier_high;
  uint32_t modifier_low;
};

// The buffer formats the desktop's compositor imports, which Chromium's GPU
// process allocates its frames in: the format table, main device and
// tranches of linux-dmabuf version 4, or the formats of version 3.
struct BufferFormats {
  uint32_t version = 0;
  int table_fd = -1;
  uint32_t table_size = 0;
  std::vector<uint8_t> main_device;
  std::vector<Tranche> tranches;
  std::vector<FormatModifier> formats;
};

struct Keymap {
  uint32_t format;
  int fd;
  uint32_t size;
};

// What Chromium learns from the desktop's compositor: the buffer formats it
// imports and the keyboard layout.
struct Offer {
  BufferFormats formats;
  std::optional<Keymap> keymap;
};

// Asks the compositor named by WAYLAND_DISPLAY, or returns nothing when there
// is none or it does not take dma-bufs.
std::optional<Offer> QueryOffer();

}  // namespace sabine_display

#endif
