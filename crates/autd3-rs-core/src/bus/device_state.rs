#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeviceState {
    Ready,
    Syncing,
    Lost,
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceState::Ready => write!(f, "READY"),
            DeviceState::Syncing => write!(f, "SYNCING"),
            DeviceState::Lost => write!(f, "LOST"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_state() {
        assert_eq!(DeviceState::Ready.to_string(), "READY");
        assert_eq!(DeviceState::Syncing.to_string(), "SYNCING");
        assert_eq!(DeviceState::Lost.to_string(), "LOST");
    }
}
