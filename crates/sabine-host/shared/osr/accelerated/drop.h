#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_DROP_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_DROP_H_

#include <string>

namespace sabine_osr {

enum class AcceleratedDropKind {
  kNone,
  kSoftwareFrames,
  kColorFormat,
  kPartialFrame,
  kWrongSize,
  kTextureSize,
  kCopyFailed,
  kHandOver,
};

// Why the last accelerated frame didn't reach the window, kept as numbers so
// a dropped frame costs nothing until the reason is reported.
struct AcceleratedDrop {
  AcceleratedDropKind kind = AcceleratedDropKind::kNone;
  int width = 0;
  int height = 0;
  int other_width = 0;
  int other_height = 0;

  std::string Describe() const;
};

}  // namespace sabine_osr

#endif
