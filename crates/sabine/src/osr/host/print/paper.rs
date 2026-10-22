use super::Paper;

#[cfg(windows)]
const LETTER: Paper = Paper {
    width_mm: 215.9,
    height_mm: 279.4,
    landscape: false,
};
#[cfg(windows)]
const A4: Paper = Paper {
    width_mm: 210.0,
    height_mm: 297.0,
    landscape: false,
};

/// The paper the default printer takes.
#[cfg(target_os = "macos")]
pub(super) fn system_default() -> Paper {
    const POINTS_PER_INCH: f64 = 72.0;
    const MILLIMETERS_PER_INCH: f64 = 25.4;
    let size = objc2_app_kit::NSPrintInfo::sharedPrintInfo().paperSize();
    let millimeters = |points: f64| points / POINTS_PER_INCH * MILLIMETERS_PER_INCH;
    Paper {
        width_mm: millimeters(size.width.min(size.height)),
        height_mm: millimeters(size.width.max(size.height)),
        landscape: false,
    }
}

/// The paper people in the person's region print on.
#[cfg(windows)]
pub(super) fn system_default() -> Paper {
    use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_IPAPERSIZE, LOCALE_RETURN_NUMBER};
    const DMPAPER_LETTER: u32 = 1;
    const DMPAPER_LEGAL: u32 = 5;
    let mut value = [0_u16; 2];
    let written = unsafe {
        GetLocaleInfoEx(
            windows::core::PCWSTR::null(),
            LOCALE_IPAPERSIZE | LOCALE_RETURN_NUMBER,
            Some(&mut value),
        )
    };
    let paper = u32::from(value[0]) | (u32::from(value[1]) << 16);
    if written > 0 && matches!(paper, DMPAPER_LETTER | DMPAPER_LEGAL) {
        LETTER
    } else {
        A4
    }
}
