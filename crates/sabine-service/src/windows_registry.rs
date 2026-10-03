use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW},
    },
    core::HSTRING,
};

pub fn current_user_string(key: &str) -> Option<String> {
    let key = HSTRING::from(key);
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            None,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let mut buffer = vec![0u16; size as usize / 2];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            None,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then(|| {
        let length = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..length])
    })
}
