#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_MACOS_SURFACE_BROKER_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_MACOS_SURFACE_BROKER_H_

#include <IOSurface/IOSurfaceRef.h>
#include <mach/mach.h>

#include <cstdint>
#include <set>
#include <string>

namespace sabine_osr {

constexpr mach_msg_id_t kSurfaceMessageId = 0x5ab1;
constexpr uint32_t kSurfaceAnnounce = 1;
constexpr uint32_t kSurfaceRetire = 2;
constexpr size_t kSurfaceTokenBytes = 64;

struct SurfaceMessage {
  mach_msg_header_t header;
  mach_msg_body_t body;
  mach_msg_port_descriptor_t surface;
  uint64_t surface_id;
  uint32_t kind;
  char token[kSurfaceTokenBytes];
};
static_assert(sizeof(SurfaceMessage) == 120,
              "the native window host reads this exact layout");

// Hands each Sabine-owned IOSurface to the native window host once, over the
// mach service it registered for this window, before frames reference it.
class SurfaceBroker {
 public:
  SurfaceBroker(std::string service_name, std::string token);
  SurfaceBroker(const SurfaceBroker&) = delete;
  SurfaceBroker& operator=(const SurfaceBroker&) = delete;
  ~SurfaceBroker();

  bool Announce(uint64_t surface_id, IOSurfaceRef surface);
  void Retire(uint64_t surface_id);

 private:
  bool Send(uint32_t kind, uint64_t surface_id, mach_port_t surface_port);

  std::string service_name_;
  std::string token_;
  mach_port_t service_ = MACH_PORT_NULL;
  std::set<uint64_t> announced_;
};

}  // namespace sabine_osr

#endif
