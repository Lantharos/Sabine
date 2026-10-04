// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The engine decodes on our own D3D11 device through a DXGI device manager and
// presents through a windowless swapchain that a composition visual shows.
// Windowless mode must be enabled before a source is set, and the swapchain
// handle stays owned by the engine.

use std::sync::{Arc, OnceLock};

use windows::Win32::{
    Foundation::HMODULE,
    Graphics::{
        Direct3D::D3D_DRIVER_TYPE_HARDWARE,
        Direct3D11::{
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
            D3D11CreateDevice, ID3D11Device, ID3D11Multithread,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
    },
    Media::MediaFoundation::{
        CLSID_MFMediaEngineClassFactory, IMFAttributes, IMFDXGIDeviceManager, IMFGetService,
        IMFMediaEngine, IMFMediaEngineClassFactory, IMFMediaEngineEx, IMFMediaEngineNotify,
        IMFMediaEngineNotify_Impl, IMFTimedText, IMFTimedTextCue, IMFTimedTextNotify,
        IMFTimedTextNotify_Impl, MF_MEDIA_ENGINE_CALLBACK, MF_MEDIA_ENGINE_DXGI_MANAGER,
        MF_MEDIA_ENGINE_TIMEDTEXT, MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, MF_TIMED_TEXT_CUE_EVENT,
        MF_TIMED_TEXT_CUE_EVENT_ACTIVE, MF_TIMED_TEXT_CUE_EVENT_CLEAR,
        MF_TIMED_TEXT_CUE_EVENT_INACTIVE, MF_TIMED_TEXT_ERROR_CODE, MF_VERSION, MFCreateAttributes,
        MFCreateDXGIDeviceManager, MFSTARTUP_FULL, MFStartup,
    },
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
};
use windows::core::{BOOL, HRESULT, Interface, Ref};
use windows_core::implement;

use super::worker::{Cue, Inbox, Report};

/// A Media Engine with the timed-text service that reports its subtitles.
pub(super) struct Engine {
    pub(super) media: IMFMediaEngineEx,
    pub(super) text: Option<IMFTimedText>,
    _devices: IMFDXGIDeviceManager,
}

impl Engine {
    pub(super) fn new(inbox: &Arc<Inbox>) -> Result<Self, String> {
        startup()?;
        unsafe { Self::create(inbox) }.map_err(|error| error.message())
    }

    unsafe fn create(inbox: &Arc<Inbox>) -> windows::core::Result<Self> {
        unsafe {
            let devices = device_manager()?;
            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 3)?;
            let attributes = attributes.ok_or_else(windows::core::Error::empty)?;
            let notify: IMFMediaEngineNotify = EngineNotify {
                inbox: Arc::clone(inbox),
            }
            .into();
            attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)?;
            attributes.SetUnknown(&MF_MEDIA_ENGINE_DXGI_MANAGER, &devices)?;
            attributes.SetUINT32(
                &MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT,
                DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32,
            )?;
            let factory: IMFMediaEngineClassFactory =
                CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
            let media: IMFMediaEngine = factory.CreateInstance(0, &attributes)?;
            let media: IMFMediaEngineEx = media.cast()?;
            media.EnableWindowlessSwapchainMode(true)?;
            let text = media
                .cast::<IMFGetService>()
                .and_then(|service| service.GetService::<IMFTimedText>(&MF_MEDIA_ENGINE_TIMEDTEXT))
                .ok();
            if let Some(text) = &text {
                let notify: IMFTimedTextNotify = TextNotify {
                    inbox: Arc::clone(inbox),
                }
                .into();
                text.RegisterNotifications(&notify)?;
            }
            Ok(Self {
                media,
                text,
                _devices: devices,
            })
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = unsafe { self.media.Shutdown() };
    }
}

fn startup() -> Result<(), String> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| {
            unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|error| error.message())
        })
        .clone()
}

/// A hardware D3D11 device for decoding, shared with the engine's threads.
unsafe fn device_manager() -> windows::core::Result<IMFDXGIDeviceManager> {
    unsafe {
        let mut device: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
        let device = device.ok_or_else(windows::core::Error::empty)?;
        let _ = device
            .cast::<ID3D11Multithread>()?
            .SetMultithreadProtected(true);
        let mut token = 0;
        let mut manager: Option<IMFDXGIDeviceManager> = None;
        MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
        let manager = manager.ok_or_else(windows::core::Error::empty)?;
        manager.ResetDevice(&device, token)?;
        Ok(manager)
    }
}

#[implement(IMFMediaEngineNotify)]
struct EngineNotify {
    inbox: Arc<Inbox>,
}

impl IMFMediaEngineNotify_Impl for EngineNotify_Impl {
    fn EventNotify(&self, event: u32, code: usize, result: u32) -> windows::core::Result<()> {
        self.inbox.post(|mail| {
            mail.reports.push(Report::Engine {
                event,
                code,
                result,
            })
        });
        Ok(())
    }
}

#[implement(IMFTimedTextNotify)]
struct TextNotify {
    inbox: Arc<Inbox>,
}

impl IMFTimedTextNotify_Impl for TextNotify_Impl {
    fn TrackAdded(&self, _track: u32) {
        self.inbox
            .post(|mail| mail.reports.push(Report::TextTracks));
    }

    fn TrackRemoved(&self, _track: u32) {
        self.inbox
            .post(|mail| mail.reports.push(Report::TextTracks));
    }

    fn TrackSelected(&self, _track: u32, _selected: BOOL) {}

    fn TrackReadyStateChanged(&self, _track: u32) {}

    fn Error(&self, _code: MF_TIMED_TEXT_ERROR_CODE, _result: HRESULT, _track: u32) {}

    fn Cue(&self, event: MF_TIMED_TEXT_CUE_EVENT, _time: f64, cue: Ref<IMFTimedTextCue>) {
        let report = match (event, cue.ok()) {
            (MF_TIMED_TEXT_CUE_EVENT_ACTIVE, Ok(cue)) => Report::Cue(unsafe { read_cue(cue) }),
            (MF_TIMED_TEXT_CUE_EVENT_INACTIVE, Ok(cue)) => Report::CueEnded(unsafe { cue.GetId() }),
            (MF_TIMED_TEXT_CUE_EVENT_CLEAR, _) => Report::CuesCleared,
            _ => return,
        };
        self.inbox.post(|mail| mail.reports.push(report));
    }

    fn Reset(&self) {
        self.inbox
            .post(|mail| mail.reports.push(Report::CuesCleared));
    }
}

/// Reads a cue's lines while the engine still holds it.
unsafe fn read_cue(cue: &IMFTimedTextCue) -> Cue {
    unsafe {
        let text = (0..cue.GetLineCount())
            .filter_map(|index| cue.GetLine(index).ok())
            .filter_map(|line| line.GetText().ok())
            .filter_map(|text| text.to_string().ok())
            .collect::<Vec<_>>()
            .join("\n");
        let start = cue.GetStartTime();
        Cue {
            id: cue.GetId(),
            text,
            start,
            end: start + cue.GetDuration(),
        }
    }
}
