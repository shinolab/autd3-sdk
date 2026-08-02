use std::path::Path;
use std::process::Command;

use autd3_rs_appliance::{
    ImageRelease, InterfaceStatus, StorageStatus, UplinkKind, UplinkStatus, WifiStatus,
};

const SYSFS_NET: &str = "/sys/class/net";
const SYSFS_RFKILL: &str = "/sys/class/rfkill";
pub const IMAGE_RELEASE: &str = "/usr/lib/autd3/image-release";
pub const DATA_DIR: &str = "/data";

fn attribute(interface: &str, name: &str) -> Option<String> {
    let path = Path::new(SYSFS_NET).join(interface).join(name);
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

#[must_use]
pub fn interface_status(interface: &str) -> InterfaceStatus {
    InterfaceStatus {
        name: interface.to_owned(),
        operstate: attribute(interface, "operstate").unwrap_or_else(|| "unknown".to_owned()),
        carrier: attribute(interface, "carrier").as_deref() == Some("1"),
        speed_mbps: attribute(interface, "speed").and_then(|speed| speed.parse().ok()),
    }
}

fn is_link_local_v6(addr: std::net::Ipv6Addr) -> bool {
    addr.segments()[0] & 0xffc0 == 0xfe80
}

fn addresses_by_interface() -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return map;
    };
    for interface in interfaces {
        let text = match interface.addr {
            if_addrs::IfAddr::V4(v4) => v4.ip.to_string(),
            if_addrs::IfAddr::V6(v6) if is_link_local_v6(v6.ip) => {
                format!("{}%{}", v6.ip, interface.name)
            }
            if_addrs::IfAddr::V6(v6) => v6.ip.to_string(),
        };
        map.entry(interface.name).or_default().push(text);
    }
    map
}

fn trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

fn rfkill_blocked(interface: &str) -> bool {
    let phy = trimmed(&Path::new(SYSFS_NET).join(interface).join("phy80211/name"));
    let Ok(entries) = std::fs::read_dir(SYSFS_RFKILL) else {
        return false;
    };
    let mut blocked = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if trimmed(&path.join("type")).as_deref() != Some("wlan") {
            continue;
        }
        let named = trimmed(&path.join("name"));
        if phy.is_some() && named.is_some() && phy != named {
            continue;
        }
        blocked |= trimmed(&path.join("soft")).as_deref() == Some("1")
            || trimmed(&path.join("hard")).as_deref() == Some("1");
    }
    blocked
}

fn iw(args: &[&str]) -> Option<String> {
    let output = Command::new("iw").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_link(text: &str) -> (Option<String>, Option<i32>) {
    let field = |name: &str| {
        let prefix = format!("{name}: ");
        text.lines()
            .find_map(|line| line.trim().strip_prefix(&prefix))
            .map(str::trim)
    };
    let signal = field("signal")
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok());
    (field("SSID").map(ToOwned::to_owned), signal)
}

fn parse_regdomain(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("country "))
        .map(|rest| rest.split(':').next().unwrap_or(rest).trim().to_owned())
        .find(|country| country.len() == 2 && country.bytes().all(|b| b.is_ascii_alphabetic()))
}

fn wifi_status(interface: &str) -> WifiStatus {
    let link = iw(&["dev", interface, "link"]).unwrap_or_default();
    let (ssid, signal_dbm) = parse_link(&link);
    WifiStatus {
        blocked: rfkill_blocked(interface),
        ssid,
        signal_dbm,
        regdomain: iw(&["reg", "get"]).as_deref().and_then(parse_regdomain),
    }
}

#[must_use]
pub fn uplinks(bus_interface: &str) -> Vec<UplinkStatus> {
    let Ok(entries) = std::fs::read_dir(SYSFS_NET) else {
        return Vec::new();
    };
    let mut addresses = addresses_by_interface();
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != "lo" && name != bus_interface)
        .filter(|name| Path::new(SYSFS_NET).join(name).join("device").exists())
        .collect();
    names.sort();

    names
        .into_iter()
        .map(|name| {
            let wireless = Path::new(SYSFS_NET).join(&name).join("wireless").exists();
            UplinkStatus {
                operstate: attribute(&name, "operstate").unwrap_or_else(|| "unknown".to_owned()),
                carrier: attribute(&name, "carrier").as_deref() == Some("1"),
                addresses: addresses.remove(&name).unwrap_or_default(),
                wifi: wireless.then(|| wifi_status(&name)),
                kind: if wireless {
                    UplinkKind::Wifi
                } else {
                    UplinkKind::Ethernet
                },
                name,
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
#[must_use]
pub fn storage(path: &str) -> Option<StorageStatus> {
    let c_path = std::ffi::CString::new(path).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a live NUL-terminated string for the duration of the call and
    // `stat` is a correctly sized `statvfs` that the call only writes to.
    if unsafe { libc::statvfs(c_path.as_ptr(), &raw mut stat) } != 0 {
        return None;
    }
    let block: u64 = stat.f_frsize;
    let mib = |blocks: libc::fsblkcnt_t| block * blocks / (1 << 20);
    Some(StorageStatus {
        path: path.to_owned(),
        total_mb: mib(stat.f_blocks),
        free_mb: mib(stat.f_bavail),
    })
}

#[cfg(not(target_os = "linux"))]
#[must_use]
pub fn storage(_path: &str) -> Option<StorageStatus> {
    None
}

fn parse_image_release(text: &str) -> ImageRelease {
    let value = |key: &str| {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| name.trim() == key)
            .map_or_else(String::new, |(_, value)| {
                value.trim().trim_matches('"').to_owned()
            })
    };
    ImageRelease {
        version: value("IMAGE_VERSION"),
        sdk_version: value("SDK_VERSION"),
        built: value("BUILT"),
        board: value("BOARD"),
        commit: value("COMMIT"),
    }
}

#[must_use]
pub fn image_release(path: &Path) -> Option<ImageRelease> {
    let text = std::fs::read_to_string(path).ok()?;
    let release = parse_image_release(&text);
    (!release.version.is_empty()).then_some(release)
}

pub fn journal_tail(unit: &str, lines: usize) -> Result<Vec<String>, String> {
    let output = Command::new("journalctl")
        .args(["-u", unit, "-n", &lines.to_string()])
        .args(["--no-pager", "-o", "short-iso"])
        .output()
        .map_err(|e| format!("failed to run journalctl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "journalctl exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_stamp_is_read_as_shell_style_key_values() {
        let release = parse_image_release(
            "# AUTD3 appliance image\nIMAGE_VERSION=\"0.4.0-20260731\"\nSDK_VERSION=0.4.0\n\
             BUILT=2026-07-31\nBOARD=raspberrypi4\nCOMMIT=1d19d23\n",
        );
        assert_eq!(release.version, "0.4.0-20260731");
        assert_eq!(release.sdk_version, "0.4.0");
        assert_eq!(release.built, "2026-07-31");
        assert_eq!(release.board, "raspberrypi4");
        assert_eq!(release.commit, "1d19d23");
    }

    #[test]
    fn a_missing_stamp_means_this_is_not_an_appliance_image() {
        assert_eq!(image_release(Path::new("/nonexistent/image-release")), None);
    }

    #[test]
    fn an_associated_link_reports_the_ssid_and_the_signal() {
        let (ssid, signal) = parse_link(
            "Connected to 3c:84:6a:00:0b:1c (on wlan0)\n\tSSID: TP-Link_0B1C\n\tfreq: 2437\n\
             \tRX: 1000 bytes (10 packets)\n\tsignal: -47 dBm\n\ttx bitrate: 72.2 MBit/s\n",
        );
        assert_eq!(ssid.as_deref(), Some("TP-Link_0B1C"));
        assert_eq!(signal, Some(-47));
    }

    #[test]
    fn an_unassociated_link_reports_nothing() {
        assert_eq!(parse_link("Not connected.\n"), (None, None));
    }

    #[test]
    fn the_regulatory_domain_is_read_from_the_country_line() {
        assert_eq!(
            parse_regdomain("global\ncountry JP: DFS-JP\n\t(2402 - 2482 @ 40), (N/A, 20)\n")
                .as_deref(),
            Some("JP"),
        );
    }

    #[test]
    fn the_world_domain_is_not_a_configured_domain() {
        assert_eq!(
            parse_regdomain(
                "global\ncountry 00: DFS-UNSET\n\t(2402 - 2472 @ 40), (N/A, 20)\n\
                 phy#0 (self-managed)\ncountry 99: DFS-UNSET\n",
            ),
            None,
        );
    }
}
