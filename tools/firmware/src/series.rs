#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Series {
    #[default]
    Sdk,
    Legacy,
}

impl Series {
    #[must_use]
    pub const fn from_flag(legacy: bool) -> Self {
        if legacy { Self::Legacy } else { Self::Sdk }
    }

    #[must_use]
    pub const fn releases_api(self) -> &'static str {
        match self {
            Self::Sdk => "https://api.github.com/repos/shinolab/autd3-sdk/releases?per_page=100",
            Self::Legacy => {
                "https://api.github.com/repos/shinolab/autd3-firmware/releases?per_page=100"
            }
        }
    }

    #[must_use]
    pub const fn tag_prefix(self) -> &'static str {
        match self {
            Self::Sdk => "firmware-v",
            Self::Legacy => "v",
        }
    }

    #[must_use]
    pub fn bundle_url(self, version: &str) -> String {
        match self {
            Self::Sdk => format!(
                "https://github.com/shinolab/autd3-sdk/releases/download/firmware-v{version}/autd3-sdk-firmware-v{version}.zip"
            ),
            Self::Legacy => format!(
                "https://github.com/shinolab/autd3-firmware/releases/download/v{version}/firmware-v{version}.zip"
            ),
        }
    }

    #[must_use]
    pub fn cache_dir_name(self, version: &str) -> String {
        match self {
            Self::Sdk => format!("autd3-sdk-firmware-v{version}"),
            Self::Legacy => format!("autd3-legacy-firmware-v{version}"),
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sdk => "autd3-sdk",
            Self::Legacy => "legacy (autd3-firmware)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_legacy_bundle_url_matches_the_published_asset() {
        assert_eq!(
            Series::Legacy.bundle_url("12.1.0"),
            "https://github.com/shinolab/autd3-firmware/releases/download/v12.1.0/firmware-v12.1.0.zip"
        );
    }

    #[test]
    fn the_sdk_bundle_url_is_unchanged() {
        assert_eq!(
            Series::Sdk.bundle_url("1.2.3"),
            "https://github.com/shinolab/autd3-sdk/releases/download/firmware-v1.2.3/autd3-sdk-firmware-v1.2.3.zip"
        );
    }

    #[test]
    fn the_two_series_never_share_a_cache_directory() {
        assert_ne!(
            Series::Sdk.cache_dir_name("12.1.0"),
            Series::Legacy.cache_dir_name("12.1.0")
        );
    }

    #[test]
    fn tag_prefixes_match_each_repository() {
        assert_eq!(Series::Sdk.tag_prefix(), "firmware-v");
        assert_eq!(Series::Legacy.tag_prefix(), "v");
        assert_eq!(Series::from_flag(false), Series::Sdk);
        assert_eq!(Series::from_flag(true), Series::Legacy);
    }
}
