const LOGGED_STEPS: f32 = 10.0;

#[derive(Debug, Default)]
pub struct ProgressLog {
    phase: String,
    logged_step: Option<u32>,
}

impl ProgressLog {
    pub fn line(&mut self, message: &str, fraction: Option<f32>) -> Option<String> {
        let step = fraction.map(|fraction| (fraction.clamp(0.0, 1.0) * LOGGED_STEPS) as u32);
        if message != self.phase {
            message.clone_into(&mut self.phase);
        } else if step <= self.logged_step {
            return None;
        }
        self.logged_step = step;
        Some(match fraction {
            Some(fraction) => format!("{message} {:.0}%", fraction.clamp(0.0, 1.0) * 100.0),
            None => message.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ProgressLog;

    #[test]
    fn logs_each_phase_once_and_every_tenth_of_the_work() {
        let mut log = ProgressLog::default();
        let lines = [
            ("Downloading runtime", 0.05),
            ("Downloading runtime", 0.06),
            ("Downloading runtime", 0.12),
            ("Downloading runtime", 0.19),
            ("Downloading runtime", 0.21),
            ("Verifying runtime", 0.72),
            ("Verifying runtime", 0.74),
            ("Runtime ready", 1.0),
        ]
        .into_iter()
        .filter_map(|(message, fraction)| log.line(message, Some(fraction)))
        .collect::<Vec<_>>();
        assert_eq!(
            lines,
            [
                "Downloading runtime 5%",
                "Downloading runtime 12%",
                "Downloading runtime 21%",
                "Verifying runtime 72%",
                "Runtime ready 100%",
            ]
        );
    }
}
