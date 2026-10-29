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

Defaults trigger IPv4 identification, collect unique vehicles for two seconds,
and use tester address 0x0E00. `DoIPConfig` and `DiscoveryConfig` provide optional
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
