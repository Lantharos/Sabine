#ifndef SABINE_RUNTIME_RENDERERS_H_
#define SABINE_RUNTIME_RENDERERS_H_

#include <string>

namespace sabine_runtime {

// Moves the renderer Chromium knows by |client_id| into |cgroup|, the unit
// of the app it draws for, so the app's memory and out-of-memory handling
// include its pages.
void AdoptRenderer(const std::string& client_id, const std::string& cgroup);

}  // namespace sabine_runtime

#endif
