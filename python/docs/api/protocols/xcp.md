# XCP

Rust-backed XCP using a packet transport, independent of its underlying CAN
or network connection. Construct `XCPClient(transport)` and call `connect()`.
`XCPOnCAN` supports CAN/CAN FD; `XCPOnTCP` and `XCPOnUDP` provide XCP Ethernet
framing. Clients, transports, configuration and enums live together in
`automotive.protocols.xcp`. `XCPTransport` exposes maximum TX/RX packet sizes
without their transport headers. Closing a transport also closes its client.

`XCPOnUDP.connect(host, port, local_addr=(local_host, local_port))` selects a
fixed local UDP endpoint. Omitting `local_addr` selects an ephemeral local
port. Both async and synchronous APIs accept this option.

Each method sends one XCP command. `upload()` and `short_upload()` read one
response; `download()` sends one packet. `get_seed()` and `unlock()` exchange
one segment per call, with the mode or remaining length supplied explicitly.
Callers handle transfer loops using the CONNECT capabilities. Sizes count bytes;
addresses count ECU elements, with negotiated byte order and address granularity.

Commands use a mutex with no background request tasks. Cancelling an active
exchange leaves the session unsynchronized; call `synchronize()` before another
command. An interrupted TCP write requires reopening the connection. Closing
waits for the active exchange to finish or reach its command deadline.

UDS and XCP share enum conversion machinery, with their own scoped definitions.
The API provides standard-mode commands, not an A2L parser, block-mode sequence
engine or background DAQ scheduler. The protocol references are ASAM XCP 1.5,
sections 7.5.1/7.5.2, and ASAM XCP on Ethernet 1.1, section 1.3.

```{eval-rst}
.. automodule:: automotive.protocols.xcp
   :members:
   :imported-members:
   :undoc-members:
```

(sync-xcp)=
## Synchronous API

Use the same configuration, result types and exceptions with ordinary method
calls under `automotive.sync.protocols.xcp`. TCP and UDP constructors block
until their transport is open. Call `connect()` separately to negotiate XCP.

```{eval-rst}
.. automodule:: automotive.sync.protocols.xcp
   :members: XCPClient, XCPTransport, XCPOnCAN, XCPOnTCP, XCPOnUDP
   :imported-members:
```
