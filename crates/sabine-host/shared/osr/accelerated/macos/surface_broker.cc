#include "osr/accelerated/macos/surface_broker.h"

#include <servers/bootstrap.h>

#include <cstring>
#include <utility>

namespace sabine_osr {

SurfaceBroker::SurfaceBroker(std::string service_name, std::string token)
    : service_name_(std::move(service_name)), token_(std::move(token)) {}

SurfaceBroker::~SurfaceBroker() {
  if (service_ != MACH_PORT_NULL) {
    mach_port_deallocate(mach_task_self(), service_);
  }
}

bool SurfaceBroker::Announce(uint64_t surface_id, IOSurfaceRef surface) {
  if (announced_.count(surface_id)) {
    return true;
  }
  const mach_port_t surface_port = IOSurfaceCreateMachPort(surface);
  if (surface_port == MACH_PORT_NULL) {
    return false;
  }
  if (!Send(kSurfaceAnnounce, surface_id, surface_port)) {
    mach_port_deallocate(mach_task_self(), surface_port);
    return false;
  }
  announced_.insert(surface_id);
  return true;
}

void SurfaceBroker::Retire(uint64_t surface_id) {
  if (announced_.erase(surface_id)) {
    Send(kSurfaceRetire, surface_id, MACH_PORT_NULL);
  }
}

bool SurfaceBroker::Send(uint32_t kind,
                         uint64_t surface_id,
                         mach_port_t surface_port) {
  if (token_.size() > kSurfaceTokenBytes) {
    return false;
  }
  if (service_ == MACH_PORT_NULL &&
      bootstrap_look_up(bootstrap_port, service_name_.c_str(), &service_) !=
          KERN_SUCCESS) {
    service_ = MACH_PORT_NULL;
    return false;
  }
  SurfaceMessage message{};
  message.header.msgh_bits =
      MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0) | MACH_MSGH_BITS_COMPLEX;
  message.header.msgh_size = sizeof(message);
  message.header.msgh_remote_port = service_;
  message.header.msgh_id = kSurfaceMessageId;
  message.body.msgh_descriptor_count = 1;
  message.surface.name = surface_port;
  message.surface.disposition = surface_port == MACH_PORT_NULL
                                    ? MACH_MSG_TYPE_COPY_SEND
                                    : MACH_MSG_TYPE_MOVE_SEND;
  message.surface.type = MACH_MSG_PORT_DESCRIPTOR;
  message.surface_id = surface_id;
  message.kind = kind;
  std::memcpy(message.token, token_.data(), token_.size());
  return mach_msg(&message.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT,
                  sizeof(message), 0, MACH_PORT_NULL, 250,
                  MACH_PORT_NULL) == MACH_MSG_SUCCESS;
}

}  // namespace sabine_osr
