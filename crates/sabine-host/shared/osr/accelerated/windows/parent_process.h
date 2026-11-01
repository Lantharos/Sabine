#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_PARENT_PROCESS_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_PARENT_PROCESS_H_

#include <cstdint>

namespace sabine_osr {

// The window host process a browser paints for. Several windows can share
// one browser process, each with its own window host, so every window keeps
// its own. Windows handle values are table-local: a handle sent over IPC must
// be created directly in the window host, or it names an unrelated object
// there.
class ParentProcess {
 public:
  ParentProcess() = default;
  ParentProcess(const ParentProcess&) = delete;
  ParentProcess& operator=(const ParentProcess&) = delete;
  ~ParentProcess();

  void Open(int pid);
  // Duplicates `handle` into the window host, with `access` or the handle's
  // own access when it is 0. Returns the handle's value there, or 0.
  uint64_t Duplicate(void* handle, uint32_t access = 0) const;
  // Closes a handle duplicated into the window host that it never received.
  void Close(uint64_t remote) const;

 private:
  void* process_ = nullptr;
};

}  // namespace sabine_osr

#endif
