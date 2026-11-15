#ifndef SABINE_RUNTIME_RENDERERS_H_
#define SABINE_RUNTIME_RENDERERS_H_

#include <string>

#include "include/cef_frame.h"

namespace sabine_runtime {

// Moves the renderer running |frame| into |cgroup|, the unit of the app it
// draws for, so the app's memory and out-of-memory handling include its
// pages. Each renderer moves once.
void AdoptRenderer(CefRefPtr<CefFrame> frame, const std::string& cgroup);

}  // namespace sabine_runtime

#endif
