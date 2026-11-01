// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Handles reach the window host by being duplicated straight into its handle
// table. The process handle that allows this stays open for the window's
// lifetime, so a window host's pid can never be confused with a process that
// reused it later.

#include "osr/accelerated/windows/parent_process.h"

#include <windows.h>

namespace sabine_osr {

ParentProcess::~ParentProcess() {
  if (process_) {
    CloseHandle(process_);
  }
}

void ParentProcess::Open(int pid) {
  if (pid > 0) {
    process_ = OpenProcess(PROCESS_DUP_HANDLE, FALSE, static_cast<DWORD>(pid));
  }
}

uint64_t ParentProcess::Duplicate(void* handle, uint32_t access) const {
  if (!process_ || !handle) {
    return 0;
  }
  HANDLE remote = nullptr;
  const DWORD options = access == 0 ? DUPLICATE_SAME_ACCESS : 0;
  if (!DuplicateHandle(GetCurrentProcess(), handle, process_, &remote, access,
                       FALSE, options) ||
      !remote) {
    return 0;
  }
  return reinterpret_cast<uint64_t>(remote);
}

void ParentProcess::Close(uint64_t remote) const {
  if (!process_ || remote == 0) {
    return;
  }
  HANDLE local = nullptr;
  DuplicateHandle(process_, reinterpret_cast<HANDLE>(remote),
                  GetCurrentProcess(), &local, 0, FALSE,
                  DUPLICATE_SAME_ACCESS | DUPLICATE_CLOSE_SOURCE);
  if (local) {
    CloseHandle(local);
  }
}

}  // namespace sabine_osr
