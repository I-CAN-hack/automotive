# DoIP

Discover vehicles with `await discover()` and connect using `await vehicle.connect()`.
For known ECUs, use `await DoIP.connect("192.168.0.10", 0x1000)`.
DoIP implements the same `Transport` used by `UDSClient`; it needs no CAN adapter.

```python
from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.doip import discover

for vehicle in await discover():
    async with await vehicle.connect() as transport:
        print(await UDSClient(transport).read_data_by_identifier(DataIdentifier.VIN))
```

Defaults trigger IPv4 identification, collect unique vehicles for two seconds
per request, and use tester address 0x0E00. Incomplete VIN/GID synchronization
with a valid VIN or GID starts a five-second per-vehicle timer. After expiry,
discovery sends another identification request and collects fresh responses.
Duplicate responses do not restart the timer; `DiscoveryConfig.timeout` controls
each response window, so synchronization can extend the total discovery time.
`DoIPConfig` and `DiscoveryConfig` provide optional
overrides. Discovery uses version 255; connections to discovered vehicles adopt
the advertised version, while direct connections default to version 2.

For passive announcements, use `async with await Discovery.listen() as listener`
and `await listener.recv()`. This binds port 13400 and sends no request.
`Discovery.bind()` instead uses a dynamic source port; `request()` triggers
identification and `recv()` receives replies. Bind before sending or powering up
an ECU. Passive IPv4 reception binds all adapters by default.

IPv6 multicast requires `DiscoveryConfig.ipv6(interface_index)`; numeric IPv6
interface indices work on Windows and Unix. Link-local endpoints retain their
scope ID (`fe80::1%7`). Ordinary TCP connections follow OS routing, with optional
source-IP binding through `DoIPConfig.local_address`. Active IPv4 discovery can
bind a source endpoint and target a subnet-directed broadcast on multihomed hosts.
Neither raw sockets nor administrator privileges are required.

For a real ECU bench on Linux, `sudo scripts/set_up_doip.sh eth0` configures
temporary static tester addresses `169.254.0.1/16` and `fe80::1/64`. Its positional
arguments accept another adapter and IPv4/IPv6 CIDRs; choose unique tester
addresses and the ECU's IPv4 subnet. From `python/`, run
`uv run examples/doip.py --ipv4 --discovery active --local-address 169.254.0.1/16` to read VIN
through UDS DID `0xF190`. Substitute `--ipv6 --adapter eth0` and/or `--discovery passive` to
exercise the other combinations. Passive mode keeps listening until Ctrl+C;
start it before powering up the ECU. Active IPv4 requires `--local-address`, e.g.
`169.254.0.1/16`, to select its source IP and derive the broadcast. Passive IPv4
needs no local address: it uses wildcard reception and TCP follows OS routing.
Allow incoming UDP port `13400` on each interface receiving announcements; the
setup script's firewall rules apply only to its selected adapter.
IPv4 needs no adapter name; IPv6 requires `--adapter` for link-local multicast.
These examples default to tester address `0x0E89` (override with
`--source-address`); the API default remains `0x0E00`.
The blocking `examples/sync/doip.py` and Rust `examples/doip.rs` take the same
arguments. The setup script marks the adapter unmanaged when NetworkManager is
running; restore it after testing with `sudo nmcli device set eth0 managed yes`.
It also adds adapter-specific `iptables`/`ip6tables` rules, when available, for
UDP announcement port `13400` and active reply ports `49152–65535`. These rules
are needed when a stateful firewall drops unicast replies to multicast/broadcast
requests; they last until firewall reload/reboot.

Alive checks are answered in the background, even when UDS is idle. Serialize
UDS exchanges on each connection. After a transport timeout, malformed TCP
message, or cancellation with a UDS reply outstanding, reconnect before issuing
another exchange. Requests are never automatically replayed. `DoIPError.code`
preserves routing activation and NACK wire codes, separately from UDS NRCs.

The transport supports unsecured TCP and physical ECU addressing. TLS,
manufacturer authentication/confirmation workflows, entity-status/power-mode
queries, periodic payload 0x8004, and functional response collection are not
implemented. A routing activation response requiring TLS or confirmation raises
`DoIPError` instead of opening an incomplete diagnostic session.

## Asynchronous API

```{eval-rst}
.. automodule:: automotive.transports.doip
   :members:
   :inherited-members:
```

(sync-doip)=
## Synchronous API

The blocking API has the same defaults, config classes, announcement fields,
and helper methods. Omit `await` and use ordinary context managers:

```python
from automotive.sync.protocols.uds import UDSClient
from automotive.sync.transports.doip import discover

for vehicle in discover():
    with vehicle.connect() as transport:
        print(UDSClient(transport).read_data_by_identifier(0xF190))
```

```{eval-rst}
.. automodule:: automotive.sync.transports.doip
   :members:
   :inherited-members:
```
