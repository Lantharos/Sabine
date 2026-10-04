#ifndef SABINE_CEF_HOST_OSR_TRANSPORT_MESSAGE_KINDS_H_
#define SABINE_CEF_HOST_OSR_TRANSPORT_MESSAGE_KINDS_H_

#include <cstdint>

// Kinds of the messages the browser sends its window, matching the window
// host's wire protocol.
constexpr uint32_t kPopupHidden = 3;
constexpr uint32_t kCursor = 4;
constexpr uint32_t kCloseRequested = 5;
constexpr uint32_t kStartDragRequested = 6;
constexpr uint32_t kMinimizeRequested = 7;
constexpr uint32_t kToggleMaximizeRequested = 8;
constexpr uint32_t kShowRequested = 9;
constexpr uint32_t kHideRequested = 10;
constexpr uint32_t kFocusRequested = 11;
constexpr uint32_t kMainBatch = 12;
constexpr uint32_t kPopupBatch = 13;
constexpr uint32_t kMainSharedBatch = 14;
constexpr uint32_t kPopupSharedBatch = 15;
constexpr uint32_t kDragStarted = 16;
constexpr uint32_t kGuestBatch = 18;
constexpr uint32_t kGuestSharedBatch = 19;
constexpr uint32_t kGuestHidden = 20;
constexpr uint32_t kDraggableRegionsChanged = 21;
constexpr uint32_t kGuestCaptureRequested = 22;
constexpr uint32_t kBridgeRequest = 23;
constexpr uint32_t kFullscreenRequested = 27;
constexpr uint32_t kExitFullscreenRequested = 28;
constexpr uint32_t kMainLoadStarted = 29;
constexpr uint32_t kMainLoadReady = 30;
constexpr uint32_t kImeStateChanged = 31;
constexpr uint32_t kImeCursorAreaChanged = 32;
constexpr uint32_t kTooltipChanged = 33;
constexpr uint32_t kImeSurroundingChanged = 34;
constexpr uint32_t kMaximizeRequested = 35;
constexpr uint32_t kRestoreRequested = 36;
constexpr uint32_t kFatalError = 37;
constexpr uint32_t kHostHello = 38;
constexpr uint32_t kDragOperation = 40;
constexpr uint32_t kContextMenu = 41;
constexpr uint32_t kContextMenuDismissed = 42;
constexpr uint32_t kFileDialog = 43;

#endif
