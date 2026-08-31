use std::net::SocketAddr;
use std::time::Duration;

use serde::de::DeserializeOwned;
use ureq::http::Uri;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

use crate::{
    Accepted, ApiError, ApplianceStatus, ConfigDocument, LogLines, ProbeResult, TuneReport,
    TuneRequest, WifiCredentials, WifiForget,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const UPDATE_TIMEOUT: Duration = Duration::from_mins(2);
const WIFI_TIMEOUT: Duration = Duration::from_mins(2);

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    #[error("cannot reach the appliance at {base}: {source}")]
    Transport {
        base: String,
        source: Box<ureq::Error>,
    },
    #[error("the appliance refused {method} {path} ({status}): {error}")]
    Api {
        method: &'static str,
        path: String,
        status: u16,
        error: String,
    },
    #[error("the appliance answered {path} with something unreadable: {source}")]
    Decode {
        path: String,
        source: serde_json::Error,
    },
}

#[derive(Debug)]
struct FixedAddr(SocketAddr);

impl Resolver for FixedAddr {
    fn resolve(
        &self,
        _uri: &Uri,
        _config: &ureq::config::Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addrs = self.empty();
        addrs.push(self.0);
        Ok(addrs)
    }
}

pub struct ApplianceClient {
    agent: ureq::Agent,
    base: String,
}

impl ApplianceClient {
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self::with_timeout(addr, DEFAULT_TIMEOUT)
    }

    #[must_use]
    pub fn with_timeout(addr: SocketAddr, timeout: Duration) -> Self {
        Self::with_resolver(format!("http://{}", host_of(addr)), Some(addr), timeout)
    }

    #[must_use]
    pub fn with_base(base: impl Into<String>) -> Self {
        Self::with_resolver(base, None, DEFAULT_TIMEOUT)
    }

    #[must_use]
    pub fn with_base_and_timeout(base: impl Into<String>, timeout: Duration) -> Self {
        Self::with_resolver(base, None, timeout)
    }

    fn with_resolver(base: impl Into<String>, addr: Option<SocketAddr>, timeout: Duration) -> Self {
        let base = base.into();
        let config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        let agent = match addr {
            Some(addr) => ureq::Agent::with_parts(config, DefaultConnector::new(), FixedAddr(addr)),
            None => {
                ureq::Agent::with_parts(config, DefaultConnector::new(), DefaultResolver::default())
            }
        };
        Self {
            agent,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn status(&self) -> Result<ApplianceStatus, ClientError> {
        self.get("/status")
    }

    pub fn config(&self) -> Result<String, ClientError> {
        Ok(self.get::<ConfigDocument>("/config")?.toml)
    }

    pub fn set_config(&self, toml: &str) -> Result<Accepted, ClientError> {
        self.put_json(
            "/config",
            &ConfigDocument {
                toml: toml.to_owned(),
            },
        )
    }

    pub fn bus_open(&self) -> Result<Accepted, ClientError> {
        self.post("/bus/open")
    }

    pub fn bus_close(&self) -> Result<Accepted, ClientError> {
        self.post("/bus/close")
    }

    pub fn bus_probe(&self) -> Result<ProbeResult, ClientError> {
        self.post("/bus/probe")
    }

    pub fn tune_start(&self, request: &TuneRequest) -> Result<Accepted, ClientError> {
        self.post_json("/bus/tune", request)
    }

    pub fn tune_report(&self) -> Result<TuneReport, ClientError> {
        self.get("/bus/tune")
    }

    pub fn tune_cancel(&self) -> Result<Accepted, ClientError> {
        self.post("/bus/tune/cancel")
    }

    pub fn tune_apply(&self, candidate: Option<usize>) -> Result<Accepted, ClientError> {
        let path = match candidate {
            Some(index) => format!("/bus/tune/apply?candidate={index}"),
            None => "/bus/tune/apply".to_owned(),
        };
        self.post(&path)
    }

    pub fn restart(&self) -> Result<Accepted, ClientError> {
        self.post("/restart")
    }

    pub fn reboot(&self) -> Result<Accepted, ClientError> {
        self.post("/reboot")
    }

    pub fn shutdown(&self) -> Result<Accepted, ClientError> {
        self.post("/shutdown")
    }

    pub fn logs(&self, unit: Option<&str>, lines: usize) -> Result<Vec<String>, ClientError> {
        let path = match unit {
            Some(unit) => format!("/logs?lines={lines}&unit={unit}"),
            None => format!("/logs?lines={lines}"),
        };
        Ok(self.get::<LogLines>(&path)?.lines)
    }

    pub fn set_wifi(&self, credentials: &WifiCredentials) -> Result<Accepted, ClientError> {
        let path = "/network/wifi";
        let response = self
            .agent
            .put(self.url(path))
            .config()
            .timeout_global(Some(WIFI_TIMEOUT))
            .build()
            .send_json(credentials);
        self.finish("PUT", path, response)
    }

    pub fn forget_wifi(&self, request: &WifiForget) -> Result<Accepted, ClientError> {
        let path = format!(
            "/network/wifi?radio_off={}&force={}",
            request.radio_off, request.force,
        );
        let response = self
            .agent
            .delete(self.url(&path))
            .config()
            .timeout_global(Some(WIFI_TIMEOUT))
            .build()
            .call();
        self.finish("DELETE", &path, response)
    }

    pub fn update(&self, binary: &[u8]) -> Result<Accepted, ClientError> {
        let path = "/update";
        let response = self
            .agent
            .post(self.url(path))
            .config()
            .timeout_global(Some(UPDATE_TIMEOUT))
            .build()
            .header("content-type", "application/octet-stream")
            .send(binary);
        self.finish("POST", path, response)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let response = self.agent.get(self.url(path)).call();
        self.finish("GET", path, response)
    }

    fn post<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let response = self.agent.post(self.url(path)).send_empty();
        self.finish("POST", path, response)
    }

    fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, ClientError> {
        let response = self.agent.post(self.url(path)).send_json(body);
        self.finish("POST", path, response)
    }

    fn put_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, ClientError> {
        let response = self.agent.put(self.url(path)).send_json(body);
        self.finish("PUT", path, response)
    }

    fn finish<T: DeserializeOwned>(
        &self,
        method: &'static str,
        path: &str,
        response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<T, ClientError> {
        let mut response = match response {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(status)) => {
                return Err(ClientError::Api {
                    method,
                    path: path.to_owned(),
                    status,
                    error: "no response body".to_owned(),
                });
            }
            Err(source) => {
                return Err(ClientError::Transport {
                    base: self.base.clone(),
                    source: Box::new(source),
                });
            }
        };
        let status = response.status().as_u16();
        let body =
            response
                .body_mut()
                .read_to_string()
                .map_err(|source| ClientError::Transport {
                    base: self.base.clone(),
                    source: Box::new(source),
                })?;

        if !(200..300).contains(&status) {
            let error = serde_json::from_str::<ApiError>(&body)
                .map_or_else(|_| body.trim().to_owned(), |parsed| parsed.error);
            return Err(ClientError::Api {
                method,
                path: path.to_owned(),
                status,
                error,
            });
        }
        serde_json::from_str(&body).map_err(|source| ClientError::Decode {
            path: path.to_owned(),
            source,
        })
    }
}

#[must_use]
pub fn host_of(addr: SocketAddr) -> String {
    match addr {
        SocketAddr::V4(v4) => v4.to_string(),
        SocketAddr::V6(v6) if v6.scope_id() != 0 => {
            format!("[{}%25{}]:{}", v6.ip(), v6.scope_id(), v6.port())
        }
        SocketAddr::V6(v6) => format!("[{}]:{}", v6.ip(), v6.port()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scoped_ipv6_address_becomes_a_legal_uri_host() {
        assert_eq!(host_of("127.0.0.1:8081".parse().unwrap()), "127.0.0.1:8081");
        assert_eq!(
            host_of("[2001:db8::1]:8081".parse().unwrap()),
            "[2001:db8::1]:8081",
        );
        assert_eq!(
            host_of("[fe80::1%3]:8081".parse().unwrap()),
            "[fe80::1%253]:8081",
            "the zone index has to be percent-encoded to survive URI parsing",
        );
    }

    #[test]
    fn the_base_url_never_ends_up_with_a_double_slash() {
        let client = ApplianceClient::with_base("http://autd3.local:8081/");
        assert_eq!(client.base(), "http://autd3.local:8081");
        assert_eq!(client.url("/status"), "http://autd3.local:8081/status");
    }
}
