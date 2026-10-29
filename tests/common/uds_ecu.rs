//! Transport setup only; UDS scenarios are independent of the selected backend.
use automotive::{Result, Stream, Transport};
use std::pin::Pin;
#[cfg(all(target_os = "linux", feature = "test-vcan"))]
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "test-vcan"))]
use automotive::{
    adapters::socketcan::{SocketCanAdapter, SocketCanConfig},
    can::Identifier,
    transports::isotp::{IsoTP, IsoTPConfig},
    StreamExt,
};

#[cfg(feature = "test-doip")]
#[path = "doip_ecu.rs"]
mod doip_ecu;

#[derive(Clone, Copy)]
pub enum Backend {
    #[cfg(all(target_os = "linux", feature = "test-vcan"))]
    Can,
    #[cfg(feature = "test-doip")]
    DoipV4,
    #[cfg(feature = "test-doip")]
    DoipV6,
}

pub enum UdsTransport {
    #[cfg(all(target_os = "linux", feature = "test-vcan"))]
    Can(IsoTP<'static>),
    #[cfg(feature = "test-doip")]
    Doip(automotive::transports::doip::DoIP),
}

impl Transport for UdsTransport {
    async fn send<'a>(&'a self, data: &'a [u8]) -> Result<()> {
        match self {
            #[cfg(all(target_os = "linux", feature = "test-vcan"))]
            Self::Can(transport) => transport.send(data).await,
            #[cfg(feature = "test-doip")]
            Self::Doip(transport) => transport.send(data).await,
        }
    }

    fn recv(&self) -> impl Stream<Item = Result<Vec<u8>>> + Unpin + '_ {
        let stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>>> + '_>> = match self {
            #[cfg(all(target_os = "linux", feature = "test-vcan"))]
            Self::Can(transport) => Box::pin(transport.recv()),
            #[cfg(feature = "test-doip")]
            Self::Doip(transport) => Box::pin(transport.recv()),
        };
        stream
    }
}

pub struct Ecu {
    pub transport: UdsTransport,
    // Keep the answering machine alive until this fixture is dropped.
    _process: tokio::process::Child,
}

impl Backend {
    pub async fn spawn(self, case: Option<&str>) -> Ecu {
        match self {
            #[cfg(all(target_os = "linux", feature = "test-vcan"))]
            Self::Can => {
                let adapter = std::sync::Arc::new(
                    SocketCanAdapter::new_async(SocketCanConfig::new("vcan0")).unwrap(),
                );
                let mut ready = adapter.recv();
                let mut command = tokio::process::Command::new(
                    std::env::var_os("PYTHON").unwrap_or_else(|| "python3".into()),
                );
                command.arg("scripts/vecu_uds.py").kill_on_drop(true);
                if let Some(case) = case {
                    command.args(["--suppression-case", case]);
                }
                let process = command.spawn().unwrap();
                tokio::time::timeout(Duration::from_secs(10), ready.next())
                    .await
                    .expect("Scapy CAN ECU did not start")
                    .expect("CAN adapter disconnected");
                drop(ready);
                let config = IsoTPConfig::builder(Identifier::standard(0x7a1).unwrap())
                    .timeout(Duration::from_secs(2))
                    .build()
                    .unwrap();
                Ecu {
                    transport: UdsTransport::Can(IsoTP::new_owned(adapter, config)),
                    _process: process,
                }
            }
            #[cfg(feature = "test-doip")]
            Self::DoipV4 | Self::DoipV6 => {
                let host = if matches!(self, Self::DoipV4) {
                    "127.0.0.1"
                } else {
                    "::1"
                };
                let ecu = doip_ecu::Ecu::spawn_with_uds_case(host, "normal", None, case).await;
                let transport = automotive::transports::doip::DoIP::connect(ecu.address, 0x1000)
                    .await
                    .unwrap();
                Ecu {
                    transport: UdsTransport::Doip(transport),
                    _process: ecu.into_process(),
                }
            }
        }
    }
}
