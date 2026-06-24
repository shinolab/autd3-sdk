use std::net::{Ipv4Addr, SocketAddr};

use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_firmware_emulator::Audit;
use autd3_rs_link_remote::{RemoteLinkOption, RemoteServer};

#[tokio::test(flavor = "multi_thread")]
async fn client_open_over_remote_emulator() {
    let mut server = RemoteServer::with_link(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        Audit::new(vec![249]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let handle = std::thread::spawn(move || server.serve_once());

    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(
        &geometry,
        RemoteLinkOption::new(addr),
        ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(client.num_devices(), 1);
    client.close().await.unwrap();
    handle.join().unwrap().unwrap();
}
