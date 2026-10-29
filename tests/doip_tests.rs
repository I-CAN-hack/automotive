#![cfg(feature = "test-doip")]

use automotive::protocols::uds::{UDSClient, UDSConfig};
use automotive::transports::doip::{
    discover_with_config, Discovery, DiscoveryConfig, DoIP, DoIPConfig, Error as DoIPError,
    VehicleIdentification,
};
use automotive::{Error, StreamExt, Transport};
use std::{
    net::{IpAddr, SocketAddr},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

#[path = "common/uds.rs"]
mod uds_services;

struct Ecu {
    _process: Child,
    address: SocketAddr,
}

impl Ecu {
    async fn spawn(host: &str, case: &str, announce_to: Option<u16>) -> Self {
        let mut command = Command::new(
            std::env::var_os("PYTHON")
                .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into()),
        );
        command
            .args(["scripts/vecu_doip.py", "--host", host, "--case", case])
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        if let Some(port) = announce_to {
            command.args(["--announce-to", &port.to_string()]);
        }
        let mut process = command
            .spawn()
            .expect("Scapy ECU requires Python and scapy==2.8.0");
        let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
        let ready = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .expect("Scapy ECU exited during startup");
        let port = ready.strip_prefix("READY ").unwrap().parse().unwrap();
        Self {
            _process: process,
            address: SocketAddr::new(host.parse::<IpAddr>().unwrap(), port),
        }
    }
}

#[tokio::test]
async fn scapy_uds_all_services_ipv4_and_ipv6() {
    for host in ["127.0.0.1", "::1"] {
        let ecu = Ecu::spawn(host, "require-idle-alive", None).await;
        let transport = DoIP::connect(ecu.address, 0x1000).await.unwrap();
        // The gateway and downstream ECU have different logical addresses.
        assert_eq!(transport.entity_address(), 0x2000);
        // Alive checks must be handled with no receive stream and no send in progress.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let uds = UDSClient::with_config(
            &transport,
            UDSConfig::builder()
                .p2_timeout(Duration::from_secs(1))
                .build(),
        );
        uds_services::exercise_services(&uds).await;
        uds.suppress_positive_responses()
            .tester_present()
            .await
            .unwrap();
        uds.tester_present().await.unwrap();
        transport.close();
    }
}

#[tokio::test]
async fn scapy_fragmented_coalesced_filtered_and_response_before_ack() {
    for case in ["fragmented", "wrong-address", "response-before-ack"] {
        let ecu = Ecu::spawn("127.0.0.1", case, None).await;
        for version in [2, 3, 4] {
            let config = DoIPConfig::builder(0x1000)
                .source_address(0x0e80)
                .protocol_version(version)
                .activation_oem([1, 2, 3, 4])
                .local_address("127.0.0.1:0".parse().unwrap())
                .build()
                .unwrap();
            let transport = DoIP::connect_with_config(ecu.address, config)
                .await
                .unwrap();
            let uds = UDSClient::with_config(
                &transport,
                UDSConfig::builder()
                    .p2_timeout(Duration::from_secs(1))
                    .build(),
            );
            assert_eq!(
                uds.read_data_by_identifier(0x1234).await.unwrap(),
                b"deadbeef"
            );
        }
    }
}

#[tokio::test]
async fn scapy_routing_rejections_preserve_codes() {
    for (case, code) in [
        ("activation-denied", 6),
        ("confirmation", 0x11),
        ("tls-required", 7),
    ] {
        let ecu = Ecu::spawn("127.0.0.1", case, None).await;
        assert!(
            matches!(DoIP::connect(ecu.address, 0x1000).await, Err(Error::DoIPError(DoIPError::RoutingActivation(actual))) if actual == code)
        );
    }
}

#[tokio::test]
async fn scapy_diagnostic_nack_is_distinct_from_uds_nrc() {
    let ecu = Ecu::spawn("127.0.0.1", "nack", None).await;
    let transport = DoIP::connect(ecu.address, 0x1000).await.unwrap();
    assert!(matches!(
        transport.send(&[0x3e, 0]).await,
        Err(Error::DoIPError(DoIPError::DiagnosticNack(3)))
    ));
}

#[tokio::test]
async fn scapy_bad_ack_header_and_size_close_connection() {
    for case in [
        "bad-ack",
        "bad-version",
        "oversize",
        "header-nack",
        "no-ack",
    ] {
        let ecu = Ecu::spawn("127.0.0.1", case, None).await;
        let config = DoIPConfig::builder(0x1000)
            .ack_timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let transport = DoIP::connect_with_config(ecu.address, config)
            .await
            .unwrap();
        let error = transport.send(&[0x3e, 0]).await.unwrap_err();
        match case {
            "oversize" => assert!(matches!(
                error,
                Error::DoIPError(DoIPError::PayloadTooLarge)
            )),
            "header-nack" => assert!(matches!(error, Error::DoIPError(DoIPError::HeaderNack(4)))),
            "no-ack" => assert!(matches!(error, Error::Timeout)),
            _ => assert!(matches!(
                error,
                Error::DoIPError(DoIPError::InvalidMessage(_))
            )),
        }
        assert!(transport.send(&[0x3e, 0]).await.is_err());
    }
}

#[tokio::test]
async fn scapy_active_and_passive_discovery_ipv4_and_ipv6() {
    for host in ["127.0.0.1", "::1"] {
        let passive = Discovery::bind(DiscoveryConfig {
            bind_address: format!("{}:0", if host == "::1" { "[::1]" } else { host })
                .parse()
                .unwrap(),
            destination: format!("{}:13400", if host == "::1" { "[::1]" } else { host })
                .parse()
                .unwrap(),
            ..Default::default()
        })
        .unwrap();
        let ecu = Ecu::spawn(
            host,
            "normal",
            Some(passive.local_address().unwrap().port()),
        )
        .await;
        let announcement = tokio::time::timeout(Duration::from_secs(2), passive.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&announcement.vin, b"WVWZZZ1JZXW000001");
        assert_eq!(announcement.logical_address, 0x1000);
        assert_eq!(announcement.vin_gid_sync_status, Some(0));
        for identification in [
            VehicleIdentification::All,
            VehicleIdentification::Vin(announcement.vin),
            VehicleIdentification::Eid(announcement.eid),
        ] {
            let vehicles = discover_with_config(DiscoveryConfig {
                bind_address: passive
                    .local_address()
                    .unwrap()
                    .ip()
                    .to_string()
                    .parse::<IpAddr>()
                    .map(|ip| SocketAddr::new(ip, 0))
                    .unwrap(),
                destination: ecu.address,
                identification,
                timeout: Duration::from_millis(150),
                ..Default::default()
            })
            .await
            .unwrap();
            assert_eq!(vehicles.len(), 1); // Bad datagrams discarded, duplicate response deduplicated.
            assert_eq!(vehicles[0].eid, announcement.eid);
        }
        let vehicles = discover_with_config(DiscoveryConfig {
            bind_address: SocketAddr::new(ecu.address.ip(), 0),
            destination: ecu.address,
            identification: VehicleIdentification::Eid([0; 6]),
            timeout: Duration::from_millis(50),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(vehicles.is_empty());
    }
}

#[tokio::test]
async fn close_wakes_receivers() {
    let ecu = Ecu::spawn("127.0.0.1", "normal", None).await;
    let transport = DoIP::connect(ecu.address, 0x1000).await.unwrap();
    let mut receiver = transport.recv();
    transport.close();
    assert!(matches!(
        receiver.next().await,
        Some(Err(Error::Disconnected))
    ));
    assert!(receiver.next().await.is_none());
}
