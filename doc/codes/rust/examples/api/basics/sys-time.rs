use core::time::Duration;

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::UdpOption;
use autd3_rs::value::SysTime;
use autd3_rs::{Client, ClientConfig};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = UdpOption::default();
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    // ANCHOR: construct
    let opened = SysTime::ZERO;
    let now = client.device_time_now()?;
    let raw = SysTime::from_nanos(1_000_000_000);
    let ns: u64 = now.sys_time();
    // ANCHOR_END: construct

    // ANCHOR: ops
    let future = client.device_time_now()? + Duration::from_millis(100);
    let past = future - Duration::from_millis(50);
    let elapsed: Duration = future - past;
    let is_after: bool = future > past; // true
    // ANCHOR_END: ops

    let _ = (opened, raw, ns, elapsed, is_after);

    client.close().await?;
    Ok(())
}
