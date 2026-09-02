use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TraceError {
    #[error("failed to open {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a pcap or pcapng capture (magic {magic:#010x})")]
    UnknownFormat { path: PathBuf, magic: u32 },
    #[error("failed to read the capture: {0}")]
    Capture(#[from] pcap_file::PcapError),
    #[error("capture link type {0} is not Ethernet")]
    NotEthernet(String),
    #[error("the capture holds no EtherCAT frames")]
    NoEtherCatFrames,
    #[error(
        "the capture holds no process-data read from {expected:#010x}, so it does not look like \
         an AUTD3 bus in OP"
    )]
    NoProcessData { expected: u32 },
    #[error("device count {found} exceeds the {max} the protocol allows")]
    TooManyDevices { found: usize, max: usize },
    #[error("device count changed from {first} to {found} at frame {frame}")]
    DeviceCountChanged {
        first: usize,
        found: usize,
        frame: usize,
    },
}
