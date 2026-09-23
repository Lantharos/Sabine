use sabine_platform::{WindowRegion, WindowRegionAdaptive, WindowRegionRect, WindowRegions};
use serde_json::Value;

use crate::{SabineLifecyclePolicy, SabineWindowControlAction, SabineWindowControlRegion};

use std::time::Duration;

pub(crate) fn regions_to_json(regions: &WindowRegions) -> Value {
    serde_json::json!({
        "blur": region_to_json(regions.blur.as_ref()),
        "opaque": region_to_json(regions.opaque.as_ref()),
        "input": region_to_json(regions.input.as_ref()),
    })
}

pub(crate) fn regions_from_json(value: Option<&Value>) -> WindowRegions {
    let Some(value) = value else {
        return WindowRegions::default();
    };
    WindowRegions {
        blur: region_from_json(value.get("blur")),
        opaque: region_from_json(value.get("opaque")),
        input: region_from_json(value.get("input")),
    }
}

pub(crate) fn rects_to_json(rects: &[WindowRegionRect]) -> Value {
    Value::Array(rects.iter().map(rect_to_json).collect())
}

pub(crate) fn rects_from_json(value: Option<&Value>) -> Vec<WindowRegionRect> {
    value
        .and_then(Value::as_array)
        .map(|rects| rects.iter().filter_map(rect_from_json).collect())
        .unwrap_or_default()
}

pub(crate) fn control_regions_to_json(regions: &[SabineWindowControlRegion]) -> Value {
    Value::Array(
        regions
            .iter()
            .map(|region| {
                serde_json::json!({
                    "action": region.action.as_str(),
                    "rect": rect_to_json(&region.rect),
                })
            })
            .collect(),
    )
}

pub(crate) fn control_regions_from_json(value: Option<&Value>) -> Vec<SabineWindowControlRegion> {
    value
        .and_then(Value::as_array)
        .map(|regions| {
            regions
                .iter()
                .filter_map(|region| {
                    Some(SabineWindowControlRegion::new(
                        SabineWindowControlAction::parse(region.get("action")?.as_str()?)?,
                        rect_from_json(region.get("rect")?)?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn lifecycle_to_json(lifecycle: &SabineLifecyclePolicy) -> Value {
    serde_json::json!({
        "active_frame_rate": lifecycle.active_frame_rate,
        "background_frame_rate": lifecycle.background_frame_rate.max(1),
        "suspend_on_minimize": lifecycle.suspend_on_minimize,
        "suspend_on_occluded": lifecycle.suspend_on_occluded,
        "suspend_on_blur": lifecycle.suspend_on_blur,
        "hibernate_after_ms": lifecycle.hibernate_after.map(duration_millis),
        "hibernate_grace_ms": duration_millis(lifecycle.hibernate_grace),
        "retain_hidden_frame": lifecycle.retain_hidden_frame,
        "memory_saver": lifecycle.memory_saver,
    })
}

pub(crate) fn lifecycle_from_json(value: Option<&Value>) -> SabineLifecyclePolicy {
    let Some(value) = value else {
        return SabineLifecyclePolicy::default();
    };
    let mut lifecycle = SabineLifecyclePolicy::default();
    lifecycle.active_frame_rate = value
        .get("active_frame_rate")
        .and_then(Value::as_u64)
        .map(|value| value.min(u32::MAX as u64) as u32)
        .unwrap_or(lifecycle.active_frame_rate);
    lifecycle.background_frame_rate = value
        .get("background_frame_rate")
        .and_then(Value::as_u64)
        .map(|value| value.max(1) as u32)
        .unwrap_or(lifecycle.background_frame_rate);
    lifecycle.suspend_on_minimize = value
        .get("suspend_on_minimize")
        .and_then(Value::as_bool)
        .unwrap_or(lifecycle.suspend_on_minimize);
    lifecycle.suspend_on_occluded = value
        .get("suspend_on_occluded")
        .and_then(Value::as_bool)
        .unwrap_or(lifecycle.suspend_on_occluded);
    lifecycle.suspend_on_blur = value
        .get("suspend_on_blur")
        .and_then(Value::as_bool)
        .unwrap_or(lifecycle.suspend_on_blur);
    lifecycle.hibernate_after = value
        .get("hibernate_after_ms")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .map(Duration::from_millis);
    lifecycle.hibernate_grace = value
        .get("hibernate_grace_ms")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .map(Duration::from_millis)
        .unwrap_or(lifecycle.hibernate_grace);
    lifecycle.retain_hidden_frame = value
        .get("retain_hidden_frame")
        .and_then(Value::as_bool)
        .unwrap_or(lifecycle.retain_hidden_frame);
    lifecycle.memory_saver = value
        .get("memory_saver")
        .and_then(Value::as_bool)
        .unwrap_or(lifecycle.memory_saver);
    lifecycle
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn region_to_json(region: Option<&WindowRegion>) -> Value {
    let Some(region) = region else {
        return Value::Null;
    };
    serde_json::json!({
        "adaptive": adaptive_to_json(region.adaptive.as_ref()),
        "rects": rects_to_json(&region.rects),
    })
}

fn rect_to_json(rect: &WindowRegionRect) -> Value {
    serde_json::json!({
        "x": rect.x,
        "y": rect.y,
        "width": rect.width,
        "height": rect.height,
    })
}

fn adaptive_to_json(adaptive: Option<&WindowRegionAdaptive>) -> Value {
    match adaptive {
        Some(WindowRegionAdaptive::Full) => serde_json::json!({ "kind": "full" }),
        Some(WindowRegionAdaptive::RoundedRect { radius }) => {
            serde_json::json!({ "kind": "rounded_rect", "radius": radius })
        }
        Some(WindowRegionAdaptive::RoundedLeft { width, radius }) => {
            serde_json::json!({ "kind": "rounded_left", "width": width, "radius": radius })
        }
        Some(WindowRegionAdaptive::TitlebarAndSidebar {
            sidebar_width,
            titlebar_height,
            radius,
        }) => {
            serde_json::json!({
                "kind": "titlebar_sidebar",
                "sidebar_width": sidebar_width,
                "titlebar_height": titlebar_height,
                "radius": radius,
            })
        }
        Some(WindowRegionAdaptive::ContentAfterSidebar {
            sidebar_width,
            titlebar_height,
        }) => {
            serde_json::json!({
                "kind": "content_after_sidebar",
                "sidebar_width": sidebar_width,
                "titlebar_height": titlebar_height,
            })
        }
        Some(WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
            sidebar_width,
            titlebar_height,
            radius,
        }) => {
            serde_json::json!({
                "kind": "content_after_sidebar_rounded_right",
                "sidebar_width": sidebar_width,
                "titlebar_height": titlebar_height,
                "radius": radius,
            })
        }
        None => Value::Null,
    }
}

fn region_from_json(value: Option<&Value>) -> Option<WindowRegion> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let adaptive = adaptive_from_json(value.get("adaptive"));
    let rects = value
        .get("rects")
        .and_then(Value::as_array)
        .map(|rects| rects.iter().filter_map(rect_from_json).collect::<Vec<_>>())
        .unwrap_or_default();
    Some(WindowRegion { rects, adaptive })
}

fn adaptive_from_json(value: Option<&Value>) -> Option<WindowRegionAdaptive> {
    let value = value?;
    match value.get("kind").and_then(Value::as_str)? {
        "full" => Some(WindowRegionAdaptive::Full),
        "rounded_rect" => Some(WindowRegionAdaptive::RoundedRect {
            radius: value.get("radius").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        "rounded_left" => Some(WindowRegionAdaptive::RoundedLeft {
            width: value.get("width").and_then(Value::as_i64).unwrap_or(0) as i32,
            radius: value.get("radius").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        "titlebar_sidebar" => Some(WindowRegionAdaptive::TitlebarAndSidebar {
            sidebar_width: value
                .get("sidebar_width")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            titlebar_height: value
                .get("titlebar_height")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            radius: value.get("radius").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        "content_after_sidebar" => Some(WindowRegionAdaptive::ContentAfterSidebar {
            sidebar_width: value
                .get("sidebar_width")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            titlebar_height: value
                .get("titlebar_height")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "content_after_sidebar_rounded_right" => {
            Some(WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
                sidebar_width: value
                    .get("sidebar_width")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                titlebar_height: value
                    .get("titlebar_height")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                radius: value.get("radius").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
        }
        _ => None,
    }
}

fn rect_from_json(value: &Value) -> Option<WindowRegionRect> {
    Some(WindowRegionRect::new(
        value.get("x")?.as_i64()? as i32,
        value.get("y")?.as_i64()? as i32,
        value.get("width")?.as_i64()? as i32,
        value.get("height")?.as_i64()? as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_round_trip_preserves_memory_saver() {
        let expected = SabineLifecyclePolicy::memory_saver_hidden_window();
        let actual = lifecycle_from_json(Some(&lifecycle_to_json(&expected)));
        assert_eq!(actual, expected);
    }
}
