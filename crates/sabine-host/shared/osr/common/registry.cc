#include "osr/common/registry.h"

#include <algorithm>
#include <mutex>
#include <vector>

namespace sabine_osr {

namespace {

std::mutex handlers_mutex;
std::vector<SabineOsrHandler*> handlers;

}  // namespace

void RegisterHandler(SabineOsrHandler* handler) {
  std::lock_guard<std::mutex> lock(handlers_mutex);
  handlers.push_back(handler);
}

void UnregisterHandler(SabineOsrHandler* handler) {
  std::lock_guard<std::mutex> lock(handlers_mutex);
  handlers.erase(std::remove(handlers.begin(), handlers.end(), handler),
                 handlers.end());
}

bool HasRegisteredHandlers() {
  std::lock_guard<std::mutex> lock(handlers_mutex);
  return !handlers.empty();
}

SabineOsrHandler* PrimaryHandler() {
  std::lock_guard<std::mutex> lock(handlers_mutex);
  return handlers.empty() ? nullptr : handlers.front();
}

}  // namespace sabine_osr
