use crate::{UPDATE_ROLLOUT_WINDOW, UPDATE_SOAK};
use sabine_runtime::sabine_data_dir;

pub(crate) fn release_is_soaked(published_at: &str) -> bool {
    let Ok(published_at) = published_at.parse::<u64>() else {
        return false;
    };
    release_age_is_soaked(
        crate::types::unix_timestamp(),
        published_at,
        update_rollout_offset(),
    )
}

fn release_age_is_soaked(now: u64, published_at: u64, rollout_offset: u64) -> bool {
    now.saturating_sub(published_at) >= UPDATE_SOAK.as_secs().saturating_add(rollout_offset)
}

fn update_rollout_offset() -> u64 {
    let path = sabine_data_dir().join("update-rollout-offset");
    if let Ok(value) = std::fs::read_to_string(&path)
        && let Ok(value) = value.trim().parse::<u64>()
    {
        return value.min(UPDATE_ROLLOUT_WINDOW.as_secs());
    }
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    let offset = u64::from_le_bytes(bytes) % UPDATE_ROLLOUT_WINDOW.as_secs();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, offset.to_string());
    offset
}

#[cfg(test)]
mod tests {
    use super::release_age_is_soaked;

    #[test]
    fn rollout_waits_for_soak_and_install_offset() {
        let day = crate::UPDATE_SOAK.as_secs();
        let offset = 90 * 60;
        assert!(!release_age_is_soaked(day + offset - 1, 0, offset));
        assert!(release_age_is_soaked(day + offset, 0, offset));
    }
}
