use autd3_rs_link_remote::{Advertisement, AdvertisementHandle, ServerKind};

const INSTANCE_PREFIX: &str = "autd3-sim";
const MAX_LABEL_LEN: usize = 63;

fn normalize(hostname: &str) -> String {
    let mut out = String::new();
    for c in hostname.split('.').next().unwrap_or_default().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

#[must_use]
pub fn instance(hostname: &str, port: u16) -> String {
    let suffix = format!("-{port}");
    let budget = MAX_LABEL_LEN - INSTANCE_PREFIX.len() - 1 - suffix.len();
    let host = normalize(hostname);
    let host = host[..host.len().min(budget)].trim_end_matches('-');
    if host.is_empty() {
        format!("{INSTANCE_PREFIX}{suffix}")
    } else {
        format!("{INSTANCE_PREFIX}-{host}{suffix}")
    }
}

#[must_use]
pub fn advertise(port: u16) -> Option<AdvertisementHandle> {
    let advertisement = Advertisement {
        instance: instance(&gethostname::gethostname().to_string_lossy(), port),
        port,
        kind: ServerKind::Simulator,
        ..Advertisement::default()
    };
    match autd3_rs_link_remote::advertise(&advertisement) {
        Ok(handle) => {
            tracing::info!(
                service = handle.fullname(),
                port,
                "advertising the simulator over mDNS",
            );
            Some(handle)
        }
        Err(err) => {
            tracing::warn!(
                %err,
                "failed to advertise the simulator over mDNS; \
                 clients have to be given the address explicitly",
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_names_the_host_and_the_port() {
        assert_eq!(instance("lab-pc", 8080), "autd3-sim-lab-pc-8080");
        assert_eq!(
            instance("lab-pc", 9000),
            "autd3-sim-lab-pc-9000",
            "two simulators on one host differ by their link port",
        );
    }

    #[test]
    fn the_hostname_is_reduced_to_a_dns_label() {
        assert_eq!(
            instance("DESKTOP-AB12CD", 8080),
            "autd3-sim-desktop-ab12cd-8080"
        );
        assert_eq!(instance("pc.lab.example.org", 8080), "autd3-sim-pc-8080");
        assert_eq!(instance("my_pc (2)", 8080), "autd3-sim-my-pc-2-8080");
        assert_eq!(instance("", 8080), "autd3-sim-8080");
        assert_eq!(instance("___", 8080), "autd3-sim-8080");
    }

    #[test]
    fn a_long_hostname_is_cut_to_fit_one_dns_label() {
        let name = instance(&"a".repeat(200), 65535);
        assert_eq!(name.len(), MAX_LABEL_LEN);
        assert!(name.starts_with("autd3-sim-aaa"));
        assert!(name.ends_with("-65535"));

        let name = instance(&format!("{}-b", "a".repeat(46)), 8080);
        assert!(!name.contains("--"), "{name}");
    }
}
