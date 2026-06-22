#[derive(Debug, thiserror::Error)]
pub enum TwinCATLinkError {
    #[error("ADS error: {0}")]
    Ads(#[from] ads::Error),
    #[error(
        "invalid device count {found} reported by the EtherCAT master; re-run the twincat-cli setup"
    )]
    InvalidDeviceCount { found: usize },
}
