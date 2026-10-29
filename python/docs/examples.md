# Examples

These are the Python programs from the repository's `python/examples/` directory.
They run against real adapters and ECUs. Select a suitable adapter and ECU address
for your setup before running them.

## Print CAN frames

Discover an adapter and print received CAN frames.

```{literalinclude} ../examples/can_printer.py
:language: python
```

## Send a CAN frame

Discover an adapter and send one Classic CAN frame.

```{literalinclude} ../examples/can_sender.py
:language: python
```

## Send ISO-TP messages

Follow the Rust example's four explicit exchanges: two short requests, a
33-byte segmented request, and another DID request that can produce a long reply.

```{literalinclude} ../examples/isotp.py
:language: python
```

## Query an ECU with UDS

Read diagnostic data through a persistent UDS client.

```{literalinclude} ../examples/uds.py
:language: python
```

## Discover vehicles and use UDS over DoIP

Trigger IPv4 identification with defaults, then connect to the advertised entity.
Optional config selects IPv6 multicast, source binding, filters, and TCP settings.

```{literalinclude} ../examples/doip.py
:language: python
```

## XCP on CAN and Ethernet

Both examples set MTA, read 4 KiB with an explicit loop of UPLOAD commands,
and print the data as hex and ASCII without writing files. CAN uses hardcoded
settings and `CanAdapter()` discovery, TX
`0x6C3`, RX `0x6C4`, and sends ten wake-up frames with ID `0x541` and eight `FF`
bytes, 100 ms apart, before connecting. The examples use straight-line code
without context managers or explicit cleanup.

CAN reads from `0x8000_0000`. Ethernet requires `--host`, `--port`, `--bind`,
`--bind-port`, and `--address` arguments, with no defaults. It reads 4 KiB over
UDP from the supplied address, using the negotiated maximum upload size and
a two-second timeout. Addresses accept hexadecimal or decimal values.

```{literalinclude} ../examples/xcp_on_can.py
:language: python
```

```{literalinclude} ../examples/xcp_on_ethernet.py
:language: python
```

## Configure bitrate and sample points

Configure a PEAK PCAN-USB FD adapter with 500 kbit/s nominal and 2 Mbit/s data
timing, both with an 80% sample point.

```{literalinclude} ../examples/configure_bitrate.py
:language: python
```

## Query firmware versions concurrently

Use independent ECU transports for concurrent diagnostic requests.

```{literalinclude} ../examples/query_fw_versions.py
:language: python
```

## Synchronous examples

These programs use the matching modules under `automotive.sync`, including
`automotive.sync.adapters`, `automotive.sync.transports.isotp`, and
`automotive.sync.protocols.uds`, with the same configuration and value types.
They use ordinary constructors, blocking calls, and receive iterators. Both the
async and sync examples let ownership release resources when they go out of scope.
Their setup, requests, output, and flow follow the corresponding Rust programs.

### Send a CAN frame

```{literalinclude} ../examples/sync/can_sender.py
:language: python
```

### Print CAN frames

```{literalinclude} ../examples/sync/can_printer.py
:language: python
```

### Send ISO-TP messages

```{literalinclude} ../examples/sync/isotp.py
:language: python
```

### Query an ECU with UDS

```{literalinclude} ../examples/sync/uds.py
:language: python
```

### Discover vehicles and use UDS over DoIP

```{literalinclude} ../examples/sync/doip.py
:language: python
```

### XCP on CAN and Ethernet

```{literalinclude} ../examples/sync/xcp_on_can.py
:language: python
```

```{literalinclude} ../examples/sync/xcp_on_ethernet.py
:language: python
```

### Configure bitrate and sample points

Configure the same PEAK timing through the synchronous API.

```{literalinclude} ../examples/sync/configure_bitrate.py
:language: python
```

### Query firmware versions

Query the same DIDs and ECU address ranges as the Rust and async Python examples.
The synchronous version visits ECUs sequentially; the async version queries them
concurrently.

```{literalinclude} ../examples/sync/query_fw_versions.py
:language: python
```

## CCP memory transfers

Connect directly to the ECU and dump the first 4 KiB of
flash. These examples configure little-endian memory addresses and print
addresses, hex bytes and ASCII using the `automotive.utils.hexdump` helper shared
with the XCP examples.

```{literalinclude} ../examples/ccp.py
:language: python
```

### Synchronous CCP

```{literalinclude} ../examples/sync/ccp.py
:language: python
```
