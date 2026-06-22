#[derive(Debug, thiserror::Error)]
pub enum TwinCATLinkError {
    #[error("ADS error: {0}")]
    Ads(#[from] ads::Error),
}
