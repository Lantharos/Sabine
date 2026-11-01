#ifndef SABINE_RUNTIME_IDENTITY_H_
#define SABINE_RUNTIME_IDENTITY_H_

#include <optional>
#include <string>

namespace sabine_runtime {

// The app on the other end of a connection to the shared runtime.
struct Peer {
  std::string cgroup;
};

// Returns the peer when it runs as the same user and its executable is the
// one Sabine registered for |app_id|, or one installed for every user under
// the system's Sabine folder, so a process cannot open another app's profile.
std::optional<Peer> VerifyPeer(int connection,
                               const std::string& app_id,
                               const std::string& data_dir);

bool ValidAppId(const std::string& app_id);

}  // namespace sabine_runtime

#endif
