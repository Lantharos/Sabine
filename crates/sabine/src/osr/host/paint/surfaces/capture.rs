use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

use crate::osr::host::events::bridge_response_line;
use crate::osr::host::native::OsrNativeHost;
use crate::render::{BgraImage, ImageId};

const NO_FRAME: &str = "guest has no frame to capture";

impl OsrNativeHost {
    /// Answers a page asking for a picture of a guest once the guest's frame
    /// is back from the GPU, without holding up the window meanwhile.
    pub(in crate::osr::host) fn capture_guest(
        &self,
        browser_id: &str,
        request_id: &str,
        guest_id: &str,
    ) {
        let control = self.control_writer.clone();
        let (browser_id, request_id) = (browser_id.to_string(), request_id.to_string());
        let respond = move |result: Result<String, String>| {
            if let Some(control) = control {
                let result = result.map(|data_url| serde_json::json!({ "dataUrl": data_url }));
                let _ = control.send(bridge_response_line(&browser_id, &request_id, result));
            }
        };
        let Some((renderer, (id, _))) = self
            .renderer
            .as_ref()
            .zip(self.overlays.get_key_value(guest_id))
        else {
            respond(Err(NO_FRAME.to_string()));
            return;
        };
        renderer.read_bgra_image_later(&ImageId::Overlay(Arc::clone(id)), move |frame| {
            std::thread::spawn(move || {
                respond(
                    frame
                        .ok_or_else(|| NO_FRAME.to_string())
                        .and_then(|frame| guest_preview_data_url(&frame)),
                );
            });
        });
    }
}

fn guest_preview_data_url(frame: &BgraImage) -> Result<String, String> {
    let (width, height) = (frame.width, frame.height);
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "guest frame is too large to capture".to_string())?;
    if width == 0 || height == 0 || frame.bytes.len() != expected_len {
        return Err(NO_FRAME.to_string());
    }
    let mut rgb = Vec::with_capacity(expected_len / 4 * 3);
    for pixel in frame.bytes.as_chunks::<4>().0 {
        let inverse_alpha = 255_u16 - u16::from(pixel[3]);
        let composite =
            |channel: u8| (u16::from(channel) + (10 * inverse_alpha + 127) / 255).min(255) as u8;
        rgb.extend_from_slice(&[
            composite(pixel[2]),
            composite(pixel[1]),
            composite(pixel[0]),
        ]);
    }
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Sub)
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|error| format!("failed to encode guest preview: {error}"))?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(&png)))
}
