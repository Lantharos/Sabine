#include "app/profile.h"

#include <filesystem>
#include <system_error>

#if defined(OS_LINUX)
#include <fcntl.h>
#include <sys/file.h>
#include <unistd.h>

#include <cerrno>
#include <climits>
#include <csignal>
#include <cstdlib>
#include <set>
#endif

namespace sabine_profile {
namespace {

namespace fs = std::filesystem;

constexpr char kSharedFolder[] = "shared";

std::string Prefix(const fs::path& directory) {
  return directory.parent_path().filename().string() + ".";
}

#if defined(OS_LINUX)
constexpr char kRuntimeLock[] = "runtime.lock";

int Lock(const fs::path& directory) {
  const fs::path path = directory.parent_path() / kRuntimeLock;
  const int fd = open(path.c_str(), O_RDWR | O_CREAT | O_CLOEXEC, 0600);
  if (fd >= 0 && flock(fd, LOCK_EX | LOCK_NB) != 0) {
    close(fd);
    return -1;
  }
  return fd;
}

bool OwnChromiumRuns(const fs::path& directory) {
  std::error_code error;
  const std::string owner =
      fs::read_symlink(directory / "SingletonLock", error).string();
  const size_t dash = owner.rfind('-');
  char host[HOST_NAME_MAX + 1] = {};
  if (error || dash == std::string::npos ||
      gethostname(host, sizeof(host) - 1) != 0 ||
      owner.substr(0, dash) != host) {
    return false;
  }
  const pid_t pid = std::atoi(owner.c_str() + dash + 1);
  return pid > 0 && (kill(pid, 0) == 0 || errno == EPERM);
}
#endif

}  // namespace

std::string CachePath(const BrowserProfile& profile, const std::string& name) {
  const fs::path own = fs::path(profile.directory) / name;
  const fs::path path =
      profile.shared.empty()
          ? own
          : fs::path(profile.shared) / (Prefix(profile.directory) + name);
  std::error_code error;
  if (path != own && !fs::exists(path, error) && fs::exists(own, error)) {
    fs::rename(own, path, error);
  }
  fs::create_directories(path, error);
  return error ? std::string() : path.string();
}

#if defined(OS_LINUX)
bool ClaimForSharedRuntime(const std::string& directory) {
  static std::set<std::string> claimed;
  if (claimed.count(directory)) {
    return true;
  }
  std::error_code error;
  fs::create_directories(directory, error);
  const int lock = Lock(directory);
  if (lock < 0) {
    return false;
  }
  if (OwnChromiumRuns(directory)) {
    close(lock);
    return false;
  }
  claimed.insert(directory);
  return true;
}

bool ReclaimFromSharedRuntime(const std::string& directory) {
  const fs::path own(directory);
  const fs::path shared = own.parent_path().parent_path() / kSharedFolder;
  std::error_code error;
  if (!fs::is_directory(shared, error)) {
    return true;
  }
  const int lock = Lock(own);
  if (lock < 0) {
    return false;
  }
  const std::string prefix = Prefix(own);
  for (const fs::directory_entry& entry :
       fs::directory_iterator(shared, error)) {
    const std::string name = entry.path().filename().string();
    if (name.rfind(prefix, 0) == 0 &&
        !fs::exists(own / name.substr(prefix.size()), error)) {
      fs::rename(entry.path(), own / name.substr(prefix.size()), error);
    }
  }
  close(lock);
  return true;
}
#endif

}  // namespace sabine_profile
