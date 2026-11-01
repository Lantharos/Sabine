#include "osr/accelerated/drop.h"

namespace sabine_osr {
namespace {

std::string Size(int width, int height) {
  return std::to_string(width) + "x" + std::to_string(height);
}

}  // namespace

std::string AcceleratedDrop::Describe() const {
  switch (kind) {
    case AcceleratedDropKind::kNone:
      return "Chromium painted nothing";
    case AcceleratedDropKind::kSoftwareFrames:
      return "Chromium paints without the GPU";
    case AcceleratedDropKind::kColorFormat:
      return "Chromium shared frames in color format " + std::to_string(width);
    case AcceleratedDropKind::kPartialFrame:
      return "Chromium's frames held only part of the page (coded " +
             Size(width, height) + ", content " +
             Size(other_width, other_height) + ")";
    case AcceleratedDropKind::kWrongSize:
      return "Chromium painted " + Size(width, height) +
             " pixels instead of the requested size";
    case AcceleratedDropKind::kTextureSize:
      return "Chromium's frame texture is " + Size(width, height) +
             " pixels, not the " + Size(other_width, other_height) +
             " it reported";
    case AcceleratedDropKind::kCopyFailed:
      return "the frame could not be copied on the GPU";
    case AcceleratedDropKind::kHandOver:
      return "the frame could not be handed to the window";
  }
  return {};
}

}  // namespace sabine_osr
