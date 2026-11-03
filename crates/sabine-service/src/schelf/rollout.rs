use std::path::Path;

/// This installation's place in Schelf's rollouts: a number below 100 that
/// it draws once and keeps, so the store never learns who is in which group.
pub(crate) fn bucket(directory: &Path) -> u8 {
    let path = directory.join("rollout");
    if let Ok(value) = std::fs::read_to_string(&path)
        && let Ok(bucket) = value.trim().parse::<u8>()
        && bucket < 100
    {
        return bucket;
    }
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    let bucket = (u64::from_le_bytes(bytes) % 100) as u8;
    let _ = std::fs::create_dir_all(directory);
    let _ = std::fs::write(path, bucket.to_string());
    bucket
}

/// Whether an installation in `bucket` takes a release offered to `percent`
/// of them, or to everyone when the release names no rollout.
pub(crate) fn includes(bucket: u8, percent: Option<u8>) -> bool {
    percent.is_none_or(|percent| bucket < percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rollout_reaches_its_share_of_installations() {
        let reached = |percent| (0..100).filter(|&bucket| includes(bucket, percent)).count();
        assert_eq!(reached(Some(0)), 0);
        assert_eq!(reached(Some(10)), 10);
        assert_eq!(reached(Some(25)), 25);
        assert_eq!(reached(Some(100)), 100);
        assert_eq!(reached(None), 100);
        assert!(includes(9, Some(10)) && !includes(10, Some(10)));
    }

    #[test]
    fn an_installation_keeps_its_bucket() {
        let directory = std::env::temp_dir().join(format!("sabine-rollout-{}", std::process::id()));
        let first = bucket(&directory);
        assert!(first < 100);
        assert_eq!(bucket(&directory), first);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
