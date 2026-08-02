use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use autd3_rs_appliance::WifiCredentials;

const HELPER: &str = "/usr/local/sbin/autd3-admin";
const STAGED_BINARY: &str = "/var/lib/autd3/staged-server";
const STAGING_TEMP: &str = "/var/lib/autd3/staged-server.tmp";

#[must_use]
pub fn staged_binary() -> PathBuf {
    PathBuf::from(STAGED_BINARY)
}

#[must_use]
pub fn staging_temp() -> PathBuf {
    PathBuf::from(STAGING_TEMP)
}

fn run(verb: &str, stdin: Option<&str>) -> Result<String, String> {
    let mut child = Command::new("sudo")
        .args(["-n", HELPER, verb])
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run {HELPER}: {e}"))?;

    if let Some(text) = stdin
        && let Some(mut pipe) = child.stdin.take()
    {
        pipe.write_all(text.as_bytes())
            .map_err(|e| format!("failed to write to {HELPER}: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for {HELPER}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{HELPER} {verb} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn reboot() -> Result<String, String> {
    run("reboot", None)
}

pub fn shutdown() -> Result<String, String> {
    run("shutdown", None)
}

pub fn install_staged() -> Result<String, String> {
    run("install", None)
}

pub fn validate_wifi(credentials: &WifiCredentials) -> Result<(), String> {
    if credentials.ssid.is_empty() {
        return Err("the SSID must not be empty".to_owned());
    }
    let empty = String::new();
    if [
        &credentials.ssid,
        credentials.psk.as_ref().unwrap_or(&empty),
        credentials.country.as_ref().unwrap_or(&empty),
    ]
    .iter()
    .any(|field| field.contains('\n'))
    {
        return Err("the SSID, passphrase and country must not contain a newline".to_owned());
    }
    if let Some(country) = &credentials.country
        && !(country.len() == 2 && country.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return Err(format!(
            "`{country}` is not a regulatory domain; it must be two letters such as JP",
        ));
    }
    Ok(())
}

pub fn set_wifi(credentials: &WifiCredentials) -> Result<String, String> {
    validate_wifi(credentials)?;
    let stdin = format!(
        "{}\n{}\n{}\n",
        credentials.ssid,
        credentials.psk.as_deref().unwrap_or(""),
        credentials.country.as_deref().unwrap_or(""),
    );
    run("wifi", Some(&stdin))
}

pub fn forget_wifi(radio_off: bool) -> Result<String, String> {
    let stdin = if radio_off { "radio-off\n" } else { "\n" };
    run("wifi-forget", Some(stdin))
}

pub fn version_of(binary: &std::path::Path) -> Result<String, String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| format!("the uploaded file did not run: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "the uploaded file exited with {} for --version",
            output.status,
        ));
    }
    accept_version(&String::from_utf8_lossy(&output.stdout))
}

fn accept_version(stdout: &str) -> Result<String, String> {
    let version = stdout.trim();
    if version.is_empty() {
        return Err("the uploaded file reported no version".to_owned());
    }
    let rest = version
        .strip_prefix(crate::BINARY_NAME)
        .and_then(|rest| rest.strip_prefix(' '))
        .ok_or_else(|| {
            format!(
                "the uploaded file reported `{version}`; \
                 only {} builds can be installed here",
                crate::BINARY_NAME,
            )
        })?;
    if rest.trim().is_empty() {
        return Err(format!(
            "the uploaded file reported `{version}`, which carries no version number",
        ));
    }
    Ok(version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("dist")
                .join(name),
        )
        .unwrap()
        .replace("\r\n", "\n")
    }

    fn assignment(script: &str, name: &str) -> String {
        script
            .lines()
            .find_map(|line| line.trim().strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("the script declares {name}="))
            .to_owned()
    }

    #[test]
    fn the_helper_installs_from_the_path_the_server_stages_to() {
        let script = dist("autd3-admin");
        assert_eq!(
            assignment(&script, "STAGED"),
            STAGED_BINARY,
            "the server would stage an update where the helper does not look for it",
        );
    }

    #[test]
    fn the_helper_never_installs_through_a_symlink() {
        let script = dist("autd3-admin");
        assert!(
            !script.contains("-o root -g root \"$STAGED\""),
            "the helper installs straight from the directory the service can write to",
        );
        assert!(
            script.contains("[ ! -L \"$pending\" ]"),
            "the helper does not reject a symlinked staging path",
        );
    }

    #[test]
    fn the_helper_does_not_inherit_the_callers_path() {
        let sudoers = dist("sudoers-autd3-admin");
        let secure_path = sudoers
            .lines()
            .find_map(|line| line.trim().strip_prefix("Defaults:autd3 secure_path="))
            .expect("the sudoers fragment pins secure_path")
            .trim_matches('"');
        assert!(
            secure_path.split(':').all(|dir| dir.starts_with('/')),
            "`{secure_path}` carries a relative entry",
        );
        assert!(
            HELPER
                .rsplit_once('/')
                .is_some_and(|(dir, _)| secure_path.split(':').any(|d| d == dir)),
            "`{secure_path}` does not cover {HELPER}",
        );
    }

    #[test]
    fn the_launcher_runs_the_binaries_the_helper_installs() {
        let helper = dist("autd3-admin");
        let launcher = dist("run-server");
        assert_eq!(
            assignment(&launcher, "IMAGE_BIN"),
            assignment(&helper, "BIN"),
        );
        assert!(
            helper.contains(&format!("BIN={}", assignment(&launcher, "DATA_BIN"))),
            "the helper does not install to the path the launcher prefers",
        );
    }

    fn read_write_paths(unit: &str) -> Vec<PathBuf> {
        unit.lines()
            .filter_map(|line| line.trim().strip_prefix("ReadWritePaths="))
            .flat_map(str::split_whitespace)
            .map(|path| PathBuf::from(path.trim_start_matches('-')))
            .collect()
    }

    #[test]
    fn the_sandbox_lets_the_helper_write_where_the_privileged_endpoints_install() {
        let helper = dist("autd3-admin");
        let paths = read_write_paths(&dist("autd3-remote-server.service"));
        for target in ["BIN", "WIFI_PROFILE", "REGDOMAIN_FILE", "RADIO_OFF_FLAG"] {
            let written = PathBuf::from(assignment(&helper, target));
            let dir = written.parent().expect("the helper writes to a file");
            assert!(
                paths.iter().any(|allowed| dir.starts_with(allowed)),
                "`{}` is read-only under `ProtectSystem=strict`, so {target} would fail with EROFS",
                dir.display(),
            );
        }
    }

    #[test]
    fn the_boot_script_reads_the_domain_the_helper_stores() {
        assert_eq!(
            assignment(&dist("autd3-admin"), "REGDOMAIN_FILE"),
            assignment(&dist("autd3-wifi-init"), "REGDOMAIN_FILE"),
            "the regulatory domain would not survive a reboot",
        );
    }

    #[test]
    fn the_boot_script_reads_the_radio_flag_the_helper_stores() {
        assert_eq!(
            assignment(&dist("autd3-admin"), "RADIO_OFF_FLAG"),
            assignment(&dist("autd3-wifi-init"), "RADIO_OFF_FLAG"),
            "the radio would come back up on the next boot",
        );
    }

    #[test]
    fn setting_the_credentials_clears_the_radio_flag() {
        let script = dist("autd3-admin");
        let set_wifi = script
            .split_once("set_wifi() {")
            .expect("the helper defines set_wifi()")
            .1;
        assert!(
            set_wifi
                .split_once("\n}\n")
                .expect("set_wifi() is a shell function")
                .0
                .contains("rm -f \"$RADIO_OFF_FLAG\""),
            "a board whose radio was turned off could never be configured again",
        );
    }

    #[test]
    fn the_helper_answers_every_verb_the_server_runs() {
        let script = dist("autd3-admin");
        for verb in ["reboot", "shutdown", "install", "wifi", "wifi-forget"] {
            assert!(
                script.contains(&format!("\n  {verb})")),
                "the helper has no case arm for `{verb}`",
            );
            assert!(
                script.contains(&format!("{verb}|")) || script.contains(&format!("|{verb}\"")),
                "`autd3-admin` does not list `{verb}` in its usage",
            );
        }
    }

    #[test]
    fn only_a_server_build_is_accepted_as_an_update() {
        assert_eq!(
            accept_version(&format!("{} 0.4.1\n", crate::BINARY_NAME)).unwrap(),
            format!("{} 0.4.1", crate::BINARY_NAME),
        );
        for foreign in [
            "autd3-appliance 0.4.0",
            "autd3-remote-server-shim 0.4.0",
            "busybox v1.36.1 (2024-01-01)",
            "0.4.0",
            crate::BINARY_NAME,
            "",
        ] {
            assert!(
                accept_version(foreign).is_err(),
                "`{foreign}` must not pass for the server binary",
            );
        }
    }

    #[test]
    fn the_staging_temp_lives_next_to_the_path_the_helper_installs_from() {
        assert_eq!(staging_temp().parent(), staged_binary().parent());
        assert_ne!(staging_temp(), staged_binary());
    }

    #[test]
    fn a_country_that_is_not_a_regulatory_domain_is_rejected() {
        for country in ["J", "JPN", "J1", "  "] {
            let credentials = WifiCredentials {
                ssid: "lab".to_owned(),
                psk: None,
                country: Some(country.to_owned()),
            };
            assert!(validate_wifi(&credentials).is_err(), "{credentials:?}");
        }
        assert!(
            validate_wifi(&WifiCredentials {
                ssid: "lab".to_owned(),
                psk: None,
                country: Some("JP".to_owned()),
            })
            .is_ok(),
        );
    }

    #[test]
    fn a_newline_in_the_credentials_cannot_forge_a_second_field() {
        for forged in [
            ("lab\nevil", None, None),
            ("lab", Some("secret\ncountry=XX"), None),
            ("lab", None, Some("JP\n")),
        ] {
            let credentials = WifiCredentials {
                ssid: forged.0.to_owned(),
                psk: forged.1.map(ToOwned::to_owned),
                country: forged.2.map(ToOwned::to_owned),
            };
            assert!(validate_wifi(&credentials).is_err(), "{credentials:?}");
        }

        assert!(
            validate_wifi(&WifiCredentials {
                ssid: String::new(),
                psk: None,
                country: None,
            })
            .is_err(),
        );
    }
}
