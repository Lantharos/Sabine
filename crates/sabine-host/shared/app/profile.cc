#include "app/profile.h"

#include <filesystem>
#include <system_error>

#if defined(OS_LINUX)
#include <fcntl.h>
#include <sys/file.h>
#include <unistd.h>

#include <set>
#endif

namespace sabine_profile {
namespace {

namespace fs = std::filesystem;

std::string Prefix(const fs::path& directory) {
  return directory.parent_path().filename().string() + ".";
}

#if defined(OS_LINUX)
constexpr char kSharedFolder[] = "shared";
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

std::string ExecutableName(const fs::path& link) {
  std::error_code error;
  const std::string name = fs::read_symlink(link, error).filename().string();
  return name.substr(0, name.find(" (deleted)"));
}

bool HostHoldsSingleton(const fs::path& directory) {
  std::error_code error;
  const std::string owner =
      fs::read_symlink(directory / "SingletonLock", error).string();
  const size_t dash = owner.rfind('-');
  if (error || dash == std::string::npos) {
    return false;
  }
  const std::string pid = owner.substr(dash + 1);
  const std::string name = ExecutableName(fs::path("/proc") / pid / "exe");
  return !name.empty() && name == ExecutableName("/proc/self/exe");
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
  if (HostHoldsSingleton(directory)) {
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

void ClearStaleSingleton(const std::string& directory) {
  const int fd = open(directory.c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC);
  if (fd < 0) {
    return;
  }
  flock(fd, LOCK_EX);
  const fs::path path(directory);
  std::error_code error;
  if (fs::is_symlink(path / "SingletonLock", error) &&
      !HostHoldsSingleton(path)) {
    for (const char* name :
         {"SingletonLock", "SingletonCookie", "SingletonSocket"}) {
      fs::remove(path / name, error);
    }
  }
  close(fd);
}
#endif

}  // namespace sabine_profile
