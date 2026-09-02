use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use pcap_file::pcap::PcapReader;
use pcap_file::pcapng::{Block, PcapNgReader};

use crate::error::TraceError;

const PCAP_MAGIC_LE: u32 = 0xa1b2_c3d4;
const PCAP_MAGIC_BE: u32 = 0xd4c3_b2a1;
const PCAP_MAGIC_NS_LE: u32 = 0xa1b2_3c4d;
const PCAP_MAGIC_NS_BE: u32 = 0x4d3c_b2a1;
const PCAPNG_MAGIC: u32 = 0x0a0d_0d0a;

#[derive(Clone, Debug)]
pub struct CapturedFrame {
    pub timestamp_ns: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureFormat {
    Pcap,
    PcapNg,
}

fn detect_format(path: &Path) -> Result<CaptureFormat, TraceError> {
    let mut file = File::open(path).map_err(|source| TraceError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|source| TraceError::Open {
            path: path.to_path_buf(),
            source,
        })?;
    let native = u32::from_le_bytes(magic);
    let swapped = u32::from_be_bytes(magic);
    if native == PCAPNG_MAGIC || swapped == PCAPNG_MAGIC {
        return Ok(CaptureFormat::PcapNg);
    }
    if matches!(
        native,
        PCAP_MAGIC_LE | PCAP_MAGIC_BE | PCAP_MAGIC_NS_LE | PCAP_MAGIC_NS_BE
    ) {
        return Ok(CaptureFormat::Pcap);
    }
    Err(TraceError::UnknownFormat {
        path: path.to_path_buf(),
        magic: native,
    })
}

fn open(path: &Path) -> Result<BufReader<File>, TraceError> {
    let mut file = File::open(path).map_err(|source| TraceError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| TraceError::Open {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(BufReader::new(file))
}

fn to_nanos(timestamp: std::time::Duration) -> u64 {
    u64::try_from(timestamp.as_nanos()).unwrap_or(u64::MAX)
}

fn read_pcap(path: &Path) -> Result<Vec<CapturedFrame>, TraceError> {
    let mut reader = PcapReader::new(open(path)?)?;
    let datalink = reader.header().datalink;
    if datalink != pcap_file::DataLink::ETHERNET {
        return Err(TraceError::NotEthernet(format!("{datalink:?}")));
    }
    let mut frames = Vec::new();
    while let Some(packet) = reader.next_packet() {
        let packet = packet?;
        frames.push(CapturedFrame {
            timestamp_ns: to_nanos(packet.timestamp),
            data: packet.data.into_owned(),
        });
    }
    Ok(frames)
}

fn read_pcapng(path: &Path) -> Result<Vec<CapturedFrame>, TraceError> {
    let mut reader = PcapNgReader::new(open(path)?)?;
    let mut frames = Vec::new();
    while let Some(block) = reader.next_block() {
        match block? {
            Block::InterfaceDescription(idb) => {
                if idb.linktype != pcap_file::DataLink::ETHERNET {
                    return Err(TraceError::NotEthernet(format!("{:?}", idb.linktype)));
                }
            }
            Block::EnhancedPacket(packet) => frames.push(CapturedFrame {
                timestamp_ns: to_nanos(packet.timestamp),
                data: packet.data.into_owned(),
            }),
            Block::SimplePacket(packet) => frames.push(CapturedFrame {
                timestamp_ns: 0,
                data: packet.data.into_owned(),
            }),
            _ => {}
        }
    }
    Ok(frames)
}

pub fn read(path: impl AsRef<Path>) -> Result<Vec<CapturedFrame>, TraceError> {
    let path = path.as_ref();
    match detect_format(path)? {
        CaptureFormat::Pcap => read_pcap(path),
        CaptureFormat::PcapNg => read_pcapng(path),
    }
}

pub fn format_of(path: impl AsRef<Path>) -> Result<CaptureFormat, TraceError> {
    detect_format(path.as_ref())
}

pub fn path_buf(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().to_path_buf()
}
