# CCP

CCP 2.1 session and memory transfer commands on Classical CAN. Construct
`CCPClient(CCPOnCAN(adapter, CCPCanConfig(...)), CCPConfig(...))`, then
`connect(station_address)` directly to a known ECU. The Rust implementation
handles fixed eight-byte CRO/CRM framing, echoed command counters,
serialization and deadlines. Standard and extended CAN identifiers are supported.

Set `CCPConfig(byte_order="little")` or `"big"` to match the ECU's memory
address encoding. CCP does not negotiate byte order. Station addresses in
CONNECT and DISCONNECT are always little endian.

Each method sends one command. `set_mta()` sets MTA0 (or MTA1), `upload()`
reads 1–5 bytes and `download()` writes 1–5 bytes, post-incrementing MTA0.
`download6()` writes exactly six bytes. Both download methods return the
ECU's post-incremented MTA0 address and extension. `short_upload()` reads
1–5 bytes at an explicit address without changing MTA0. Sizes count bytes.
Callers implement transfer loops and seed/key algorithms. Optional commands
remain subject to ECU support and resource protection.

`get_ccp_version()` exchanges versions; `exchange_id()` reports resource
masks and sets MTA0 to the identification, retrieved using `upload()`.
`get_seed()` accepts one CCP resource (CAL=1, DAQ=2, PGM=0x40), and
`unlock()` sends up to six key bytes.

Commands are never retried automatically. Busy responses wait for ACK within
the original deadline. After active cancellation, timeout, malformed response
or an interrupting ECU event, reconnect and re-establish MTA and privileges.
The interrupted command may already have executed. Negative responses raise
`CCPResponseError` with raw `command`, `code`, and `data` attributes.
Closing waits for the active exchange and releases the local transport;
`disconnect()` separately ends the ECU session.

This client implements CCP 2.1 sections 7, 12.1–12.9, 12.15 and 12.28.

```{eval-rst}
.. automodule:: automotive.protocols.ccp
   :members:
   :imported-members:
   :undoc-members:
```

(sync-ccp)=
## Synchronous API

Use identical configuration, values and exceptions through ordinary methods
under `automotive.sync.protocols.ccp`.

```{eval-rst}
.. automodule:: automotive.sync.protocols.ccp
   :members: CCPClient, CCPTransport, CCPOnCAN
   :imported-members:
```
