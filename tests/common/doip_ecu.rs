use std::{
    net::{IpAddr, SocketAddr},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

pub struct Ecu {
    _process: Child,
    pub address: SocketAddr,
}

impl Ecu {
    #[allow(dead_code)]
    pub fn into_process(self) -> Child {
        self._process
    }

    #[allow(dead_code)]
    pub async fn spawn(host: &str, case: &str, announce_to: Option<u16>) -> Self {
        Self::spawn_with_uds_case(host, case, announce_to, None).await
    }

    pub async fn spawn_with_uds_case(
        host: &str,
        case: &str,
        announce_to: Option<u16>,
        uds_case: Option<&str>,
    ) -> Self {
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
        if let Some(case) = uds_case {
            command.args(["--suppression-case", case]);
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
