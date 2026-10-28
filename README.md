# The Automotive Crate
[![crates.io](https://img.shields.io/crates/v/automotive.svg)](https://crates.io/crates/automotive)
[![docs.rs](https://img.shields.io/docsrs/automotive)](https://docs.rs/automotive/latest/automotive/)
[![PyPI](https://img.shields.io/pypi/v/automotive.svg)](https://pypi.org/project/automotive/)

Welcome to the `automotive` crate documentation. The purpose of this crate is to help you with all things automotive related. Most importantly, it provides a fully async CAN interface supporting multiple adapters.

## Modules

- `automotive::adapters` contains the feature-gated hardware adapters: `j2534`,
  `panda`, `peak`, `socketcan`, and `vector`, plus the `get_adapter()` discovery
  helper.
- `automotive::transports` contains ISO-TP and the diagnostic `Transport` interface.
- `automotive::protocols` contains `uds`, which uses a `Transport`, and `xcp`,
  which exchanges XCP packets over raw CAN/CAN FD or Ethernet TCP/UDP.
- `automotive::can` provides shared CAN types and traits.

Hardware adapter types use the `Adapter` suffix: `PandaAdapter`, `PeakAdapter`,
`SocketCanAdapter`, `VectorAdapter`, and `J2534Adapter`. Message transports
implement `Transport`; the current implementations are `IsoTP` and
`J2534NativeIsoTP`. UDS depends on this shared interface so additional transports,
such as DoIP and HSFZ, can implement it independently of CAN.

## Python library

A Python package with synchronous and asyncio APIs is available in `python/`,
with the import name `automotive`. See the [Python README](python/README.md) for
installation, configuration, examples, API documentation, and Scapy ECU
regression tests.
The [Python documentation](https://i-can-hack.github.io/automotive/) includes its
user guide, examples, and API reference.

## Development with Nix

Enter the development environment and run the tests:

```sh
nix develop
cargo test --features all
```

The shell includes Rust, Cargo, rustfmt, Clippy, rust-analyzer, libusb,
libclang for bindgen, maturin, uv, and Python with Scapy. On Linux it also
includes the CAN and network tools used by the virtual ECU tests. The flake lock
and release checksums pin the packages.

To run the virtual CAN tests on Linux, create `vcan0` from the repository root:

```sh
scripts/set_up_vcan.sh
cargo test --features test-vcan,serde
```

The UDS tests exercise all 14 implemented service IDs against Scapy's
`EcuAnsweringMachine`, including DID and memory writes followed by reads, and
download/upload transfers. Scapy decodes requests and constructs responses.

The setup script requires `sudo`. Windows-only adapters still require their
Windows SDKs and drivers.

## Async CAN Example

The [CAN sender example](examples/can_sender.rs) sends one Classic CAN frame
through the first available adapter:

```sh
cargo run --example can_sender --features default-adapters
```

The following adapter opens the first available adapter on the system, and then receives all frames. Note how the sent frame is awaited, which waits until the message is ACKed on the CAN bus.

```rust
use automotive::StreamExt;
async fn can_example() -> automotive::Result<()> {
    let adapter = automotive::adapters::get_adapter()?;
    let mut stream = adapter.recv();

    let frame = automotive::can::Frame::builder(0x541.try_into()?).data(&[0xff; 8]).build()?;
    adapter.send(&frame).await?;

    while let Some(frame) = stream.next().await {
        let Some(id) = frame.id() else { continue };
        let id = id.as_raw();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data()));
    }
    Ok(())
}
```

`AsyncCanAdapter::recv_for(duration).await` collects frames for a `Duration`
and returns a `Vec<Frame>` in receive order, including transmit echoes. Its
independent subscription closes on return or cancellation; existing receive
streams are unaffected. Reception starts when the future is polled and ends
early if the adapter closes. Raw adapters expose the blocking
`CanAdapter::recv_for(duration) -> Result<Vec<Frame>>`; their normal receive
timeout can extend the collection past the requested duration.

## Building CAN frames

Use the data-frame builder or the remote/error constructors below. The builder
borrows a payload slice until `build()`, then copies it into the frame's inline
storage. Construction checks the length without padding or truncating.

```rust
use automotive::can::{ErrorFlag, Frame, Identifier};

# fn example() -> automotive::Result<()> {
let id = Identifier::standard(0x123)?;
let classic = Frame::builder(id).data(&[1, 2, 3]).build()?;
let fd = Frame::builder(id).fd(true).brs(true).data(&[0xaa; 12]).build()?;
let remote = Frame::remote(1, id, 8)?;
let error = Frame::error(0, ErrorFlag::Active);
# Ok(())
# }
```

Classic frames accept zero to eight bytes. FD frames accept lengths 0–8, 12,
16, 20, 24, 32, 48, and 64, including short FD frames. The builder rejects
`brs` and `esi` on Classic frames. Remote requests have a four-bit DLC and no
payload; error frames have an active or passive error flag and no arbitration
ID, DLC, or payload.
Error frames can be represented, but adapters do not currently transmit them.
Adapters return an error for frame types or options their protocol cannot encode.
BRS is selected per frame and requires a configured data bitrate on PEAK adapters.

Frame fields `id`, `data`, and `fd` are now accessed through `id()`, `data()`,
and `is_fd()`. `id()` returns `None` for error frames. Use `content()` to match on
`FrameContent` and access type-specific fields. Identifiers use the
`Identifier::standard(...)?` and `Identifier::extended(...)?` constructors,
which check their ranges immediately. The enum variants contain wrappers with
private numeric fields, so their checks cannot be bypassed.
An explicitly extended ID keeps its format even when its value fits in 11 bits.
Serde deserialization applies the same validation, using the new
`content` enum in place of the old flat frame fields.

## Adapter configuration

`BitrateBuilder` produces a `BitrateConfig` containing nominal and optional CAN-FD
data-phase timings. Adapter settings are supplied separately to each adapter's
`new` or `new_async` constructor:

- `PeakConfig::new(bitrate)` and `PandaConfig::new(bitrate)` require bus timing.
- `SocketCanConfig::new("can0")` selects an existing interface; bus timing is
  configured through the operating system.
- `VectorConfig::new(channel)` opens a channel with shared access and retains its
  existing bus timing. The `VectorAccess` enum ties initialization access to a
  required bitrate.

The configs also provide `builder(...)` constructors. PEAK, panda, and Vector
builders accept a nested bitrate builder through `.bitrate(|bitrate| ...)`,
automatically selecting the adapter's timing capabilities. Vector requests
initialization access when a bitrate is supplied; omitting `.bitrate(...)` keeps
shared access. SocketCAN's builder selects an existing interface.

Omitting `.bitrate(...)` on PEAK or panda uses 500 kbit/s nominal and 2 Mbit/s
data, with an 80% sample point for both phases:

```rust,no_run
# #[cfg(feature = "peak")]
# fn example() -> automotive::Result<()> {
let config = automotive::adapters::peak::PeakConfig::builder().build()?;
let adapter = automotive::adapters::peak::PeakAdapter::new_async(config)?;
# Ok(())
# }
```

An explicit `.bitrate(...)` callback starts without preset timing. Supplying only
a nominal bitrate produces no data-phase timing; direct timing parameters are also
supported. `get_adapter()` uses the same defaults when configuring PEAK, panda,
or Vector.

PEAK leaves kernel drivers attached by default. Opening the raw-USB adapter
fails if a driver such as `peak_usb` already owns the USB interface. Set
`.detach_kernel_driver(true)` on the PEAK config builder to allow detaching
the driver while the adapter is open. Automatic discovery uses the default
and leaves SocketCAN devices available.

For example, to configure a PEAK adapter without selecting a generic adapter
type for the bitrate builder:

```rust,no_run
# #[cfg(feature = "peak")]
# fn example() -> automotive::Result<()> {
use automotive::adapters::peak::{PeakAdapter, PeakConfig};

let config = PeakConfig::builder()
    .bitrate(|bitrate| {
        bitrate
            .bitrate(500_000)
            .sample_point(0.8)
            .data_bitrate(2_000_000)
            .data_sample_point(0.8)
    })
    .build()?;
let adapter = PeakAdapter::new_async(config)?;
# Ok(())
# }
```

Run the complete PEAK bitrate example with
`cargo run --example configure_bitrate --features peak`.

Each adapter's configuration types live in its `config.rs` module and are
re-exported from the adapter module.

## Transmit timeouts

Vector and PEAK default to a one-second transmit timeout. Set it using the
adapter config builder's `.tx_timeout(Duration)` method, or disable it with
`.without_tx_timeout()`. Their configs expose the corresponding `tx_timeout`
field; `None` disables the deadline. A zero timeout is rejected with
`Error::InvalidAdapterConfig` when opening the adapter. Panda, SocketCAN and
J2534 do not support TX cancellation and expose no TX timeout setting.

`adapter.send(&frame).await` returns `Result<()>`. `Error::Timeout` means the
send was still unconfirmed when its expired deadline was checked. A matching TX
confirmation received before cancellation succeeds even if its deadline has
passed. `Error::Cancelled` means the send was invalidated by a transmit queue
clear. Adapter errors retain their original error variant. A queue clear
invalidates all pending sends, including software-queued frames and callers
waiting for queue capacity. ISO-TP and UDS propagate CAN transmit failures.

Supported adapters also clear TX on startup. PEAK holds the controller in reset,
uses TX_ABORT/FLUSH, drains old transmit echoes, and restores bus operation. This
briefly interrupts reception on its channel; received non-TX frames are buffered
while draining. USB firmware does not need to emit an RX-barrier reply. A fresh
PEAK adapter is cleared once before the bus is enabled.

Vector sends the first real frame with HIGHPRIO, which clears its send buffers
before transmitting that frame; after a timeout the next real frame uses
HIGHPRIO again. Until that replacement is submitted, old Vector frames can still
be pending in hardware.

Unmatched echoes are warned about and ignored until the first current TX
confirmation. After that, an unmatched echo is an adapter invariant violation and
panics the worker. Every queue clear resets this startup/recovery grace period.

## ISO-TP configuration

`IsoTPConfig::builder(tx_id)` follows the same consuming-setter pattern as the
frame and adapter builders. Only the transmit identifier is required:

```rust
use automotive::can::Identifier;
use automotive::transports::isotp::IsoTPConfig;

# fn example() -> automotive::Result<()> {
let config = IsoTPConfig::builder(Identifier::standard(0x7a1)?).build()?;
assert_eq!(config.rx_id, Identifier::standard(0x7a9)?);
# Ok(())
# }
```

Empty ISO-TP payloads are rejected. Transport operations also validate public
config fields before framing, including configs modified after `build()`.

Defaults are bus zero, Classical CAN, padding enabled, a one-second transport
timeout, unlimited receive blocks, and zero receive STmin.
Transmitted consecutive frames follow the peer's block size and
STmin; `.separation_time_min(...)` overrides transmit STmin.
With zero transmit STmin, consecutive frames are queued in bounded batches
within the peer's block limit. Every frame is confirmed before completing the
block or transfer, without adding a timer delay between frames.
Batching is enabled by default. Set `.batch_send(false)` for adapters that
reorder queued frames; each consecutive frame is then confirmed before
submitting the next one.

`.timeout(...)` applies independently to CAN transmit confirmation (N_As/N_Ar),
required flow control (N_Bs), consecutive frames (N_Cr), and idle reception.
The one-second default follows ISO 15765-2:2016 Table 21. N_Bs starts after the
preceding data frame is confirmed; N_Cr starts after flow control is confirmed
and restarts on each valid consecutive frame. Ignored PDUs do not restart timers.
Idle reception expiry yields `Error::IdleTimeout`; poll the same stream again
to resume waiting. Timeouts during transmission or reassembly yield
`Error::Timeout`.

Receive IDs are derived by adding eight to a standard ID or swapping the low two
bytes of an extended ID (for example, `0x18DA10F1` becomes `0x18DAF110`). These
are convenience conventions: ISO 15765-2:2016 §10.3.2 leaves normal addressing
mappings application-specific; §10.3.3 defines the address bytes for normal fixed
addressing. Use `.rx_id(...)` for arbitrary mappings or `.rx_id_offset(...)` for
a different standard-ID offset. The last receive ID/offset setter wins, and
`build()` reports overflowing derived IDs as errors.

For a custom receive ID, bus, and timeout, or a different standard-ID offset:

```rust
use automotive::can::Identifier;
use automotive::transports::isotp::IsoTPConfig;
use std::time::Duration;

# fn example() -> automotive::Result<()> {
let explicit = IsoTPConfig::builder(Identifier::standard(0x7ff)?)
    .rx_id(Identifier::standard(0x650)?)
    .bus(1)
    .timeout(Duration::from_secs(1))
    .without_padding()
    .build()?;

let offset = IsoTPConfig::builder(Identifier::standard(0x700)?)
    .rx_id_offset(0x6a)
    .build()?;
assert_eq!(offset.rx_id, Identifier::standard(0x76a)?);
# Ok(())
# }
```

Use `.fd(true)` for CAN FD. TX_DL defaults to eight bytes for Classical CAN or
64 for FD; `.max_dlen(...)` selects a different transmit length, including any
address byte. The builder checks §9.5.1 values: 8 for Classical CAN, or 8, 12,
16, 20, 24, 32, 48, and 64 for FD. `.padding(byte)` changes optional padding;
`.without_padding()` disables it, while FD still pads frames to a valid DLC.

For CAN FD, BRS defaults to enabled when a data bitrate is configured. Timing
that is unknown or managed externally, such as SocketCAN or shared Vector
channels, also defaults to BRS enabled. `.brs(true)` or `.brs(false)` explicitly
enables or disables it for all outgoing frames, including flow control.
Enabling BRS requires `.fd(true)` and adapter support.

Receive data length is learned from each incoming FirstFrame, independently of
the local transmit length. Invalid SF/FF length encodings and CAN data lengths
are discarded; non-final consecutive frames must match the peer's FirstFrame
length. This follows ISO 15765-2:2016 §9.5.3–§9.6.4.

If a valid SF or FF interrupts a segmented reception, `recv()` yields
`isotp::Error::UnexpectedPdu` and then processes the replacement message.
Idle consecutive frames and unknown PDUs are ignored (§9.8.3, Table 23).

Received FC.WAIT frames restart the sender's flow-control deadline. There is no
fixed sender-side WAIT count; N_WFTmax applies to a receiver sending WAITs.
A reserved STmin encoding selects the required 127 ms fallback for the remainder
of that transfer (§9.6.5.5, §9.7).

`.ext_address(byte)` inserts and expects an address byte for extended or mixed
addressing (§10.3.4–§10.3.5). `.rx_ext_address(byte)` overrides the receive byte
when the two directions use different addresses. Receive block size and STmin
are advertised in flow-control frames (§9.6.5); STmin rounds up to a wire value
and values above 127 ms are rejected. The frame timeout must be nonzero.
Fields with Serde defaults may be omitted from serialized configs.

For CAN FD with a 32-byte transmit length and receive flow control:

```rust
use automotive::can::Identifier;
use automotive::transports::isotp::IsoTPConfig;
use std::time::Duration;

# fn example() -> automotive::Result<()> {
let config = IsoTPConfig::builder(Identifier::extended(0x18da10f1)?)
    .fd(true)
    .max_dlen(32)
    .block_size(8)
    .receive_separation_time_min(Duration::from_micros(300))
    .build()?;
assert_eq!(config.rx_id, Identifier::extended(0x18daf110)?);
# Ok(())
# }
```

The `IsoTPConfig` API documentation also includes an extended-addressing example
with different transmit and receive address bytes.

## XCP

XCP over CAN, CAN FD, and Ethernet (TCP/UDP) is supported through
`automotive::protocols::xcp`, with matching asynchronous and synchronous Python APIs.

## UDS Example
The automotive crate also supplies interfaces for various diagnostic protocols such as UDS. The adapter is first wrapped to support the ISO Transport Layer, then a UDS Client is created. All methods are fully async, making it easy to communicate with multiple ECUs in parallel. See [automotive#21](https://github.com/I-CAN-hack/automotive/issues/21) for progress on the supported SIDs.

```rust
 async fn uds_example() -> automotive::Result<()> {
    let adapter = automotive::adapters::get_adapter()?;
    let isotp = automotive::transports::isotp::IsoTP::from_id(&adapter, 0x7a1);
    let uds = automotive::protocols::uds::UDSClient::new(&isotp);

    uds.tester_present().await?;
    let response = uds.read_data_by_identifier(automotive::protocols::uds::DataIdentifier::ApplicationSoftwareIdentification).await?;

    println!("Application Software Identification: {}", hex::encode(response));
    Ok(())
 }
```

UDS service methods, including those on the suppressed client view, accept
domain types or raw numbers directly, without casts or `.into()` calls:

```rust
use automotive::protocols::uds::{
    CompressionMethod, DataIdentifier, EncryptionMethod, ResetType,
    RoutineControlType, RoutineIdentifier, SecurityAccessType, UDSClient,
};

# async fn typed_uds_example<T: automotive::Transport>(uds: &UDSClient<'_, T>) -> automotive::Result<()> {
uds.ecu_reset(ResetType::HardReset).await?;
uds.ecu_reset(0x1).await?;
uds.security_access(SecurityAccessType::NonStandard(0x11), None).await?;
uds.read_data_by_identifier(DataIdentifier::Vin).await?;
uds.read_data_by_identifier(0xf190).await?;
uds.routine_control(RoutineControlType::Start, RoutineIdentifier::EraseMemory, None).await?;
uds.routine_control(0x1, 0xff00, None).await?;

uds.request_download_alfid(CompressionMethod::NONE, EncryptionMethod::NONE, 0x1234, 0x10, 2, 1).await?;
uds.request_upload_alfid(0, 0, 0x1234, 0x10, 2, 1).await?;
uds.request_download_alfid(CompressionMethod::new(0xa)?, 0x3, 0x1234, 0x10, 2, 1).await?;
# Ok(())
# }
```

Code enums use named variants for known values and `NonStandard(u8)` or
`NonStandard(u16)` for other values. `from_raw()` and `From` decode numbers
without losing unknown values; `as_raw()` and conversion back to the wire
integer preserve all bits. Iteration visits only named variants. Enum equality
compares variants; compare `as_raw()` values when comparing wire encodings.
Counts, masks, counters, addresses, sizes, and byte buffers use numeric or
byte-slice representations.

Compression and encryption use separate validated types with `NONE` constants.
Each holds an unshifted value in `0..=0xf`; compression occupies the high nibble
and encryption the low nibble of the data format identifier (`0xa`, `0x3`
encode as `0xa3`). Both transfer methods accept any mix of typed methods and raw
numbers. Out-of-range inputs return `uds::Error::InvalidCompressionMethod` or
`InvalidEncryptionMethod` before sending. Checked construction also applies to
Serde deserialization.

Memory reads, writes, downloads, and uploads have `_alfid` variants that accept
`u64` addresses and sizes. The two width arguments are **address width first,
size width second**, each in bytes from 1 to 8. The client encodes the ALFID and
big-endian fields, preserving leading zeros. A value that does not fit returns
`uds::Error::MemoryValueOverflow`; an invalid width returns
`InvalidMemoryValueWidth` before sending.

```rust
use automotive::protocols::uds::UDSClient;

# async fn memory_example<T: automotive::Transport>(uds: &UDSClient<'_, T>) -> automotive::Result<()> {
// A three-byte address and a two-byte size.
let data = uds.read_memory_by_address_alfid(0x123456, 8, 3, 2).await?;
uds.write_memory_by_address_alfid(0x123456, 8, 3, 2, &data).await?;
uds.request_download_alfid(0, 0, 0x123456, 8, 3, 2).await?;
# Ok(())
# }
```

The methods without the `_alfid` suffix accept big-endian byte slices. For fixed
integer widths, `0x0010_0000_u32.to_be_bytes()` produces a four-byte address and
`0x2000_u16.to_be_bytes()` produces a two-byte size. These arrays can be passed
by reference to all four byte-slice methods.

`UDSClient::new` uses strict response validation and default timing settings.
Use `UDSConfig::builder()` and `UDSClient::with_config` to customize the settings:

```rust
use automotive::protocols::uds::{UDSClient, UDSConfig};
use std::time::Duration;

# async fn configured_uds_example<T: automotive::Transport>(transport: &T) -> automotive::Result<()> {
let config = UDSConfig::builder()
    .p2_timeout(Duration::from_millis(100)) // Default: 50 ms.
    .build();
let uds = UDSClient::with_config(transport, config);
uds.tester_present().await?;
# Ok(())
# }
```

The default response waits are P2 = 50 ms and P2* = 5 s, based on the recommended
server timings in ISO 14229-2:2013, §7.2, Table 4. Configured and server-reported
timeouts are used directly. P2 starts after sending the request. Each Response
Pending (NRC 0x78) restarts P2*. By default there is no overall request limit;
`.request_timeout(...)` caps the entire exchange,
including sending and pending responses. `.without_request_timeout()` removes
that cap. Expired deadlines return `automotive::Error::Timeout`, except for
expected silence when a positive response is suppressed.

Use a scoped view to suppress positive responses without changing the original
client's behavior:

```rust
# async fn suppressed_uds_example<T: automotive::Transport>(transport: &T) -> automotive::Result<()> {
use automotive::protocols::uds::{RequestOutcome, ResetType, UDSClient};

let uds = UDSClient::new(transport);
let report = uds
    .read_dtc_information_number_of_dtc_by_status_mask(0xff)
    .await?;
println!("DTC count: {}", report.dtc_count);
{
    let suppressed = uds.suppress_positive_responses();
    suppressed.tester_present().await?; // Sends [0x3e, 0x80].
    suppressed.ecu_reset(ResetType::HardReset).await?;
    suppressed.ecu_reset(0x1).await?; // Both send [0x11, 0x81].

    // All typed helpers are available on the view.
    let report = suppressed
        .read_dtc_information_number_of_dtc_by_status_mask(0xff)
        .await?; // Sends [0x19, 0x81, 0xff]; silence returns None.
    let data = suppressed.read_data_by_identifier(0xf181).await?;
    // ReadDataByIdentifier has no sub-function and still requires a response.

    // Custom requests distinguish silence from an empty positive response.
    match suppressed.request(0x3e, Some(0), None).await? {
        RequestOutcome::Suppressed => println!("No response received"),
        RequestOutcome::PositiveResponse(data) => println!("Response: {data:02x?}"),
    }
}
// The raw API also honors the suppression bit directly, without a scoped view.
uds.request(0x3e, Some(0x80), None).await?;
uds.tester_present().await?; // Sends [0x3e, 0x00] and requires a response.
# Ok(())
# }
```

`SuppressedUDSClient` borrows the original client and sets bit 7 of the
sub-function byte. It provides suppression-aware helpers for every implemented
service with a sub-function. Services without a sub-function are available
through the wrapper and still require a response. Both views share session
timings. Keep one request outstanding per ECU.

Suppression still waits through the initial response window to catch negative
responses and space sequential requests (ISO 14229-2:2013, §8.3). Initial silence
returns `RequestOutcome::Suppressed` from `request()`; a received positive
response returns `RequestOutcome::PositiveResponse(data)`. Typed helpers
represent silence as `None`, or `()` for TesterPresent. `security_access()`
returns `Some(seed)` for a received seed and `None` for SendKey or suppressed
silence. Normal `read_dtc_information_number_of_dtc_by_status_mask()` calls
return a report directly. On the suppressed view, this helper returns `None`
for silence and `Some(report)` for a received report. Silence does not confirm
that the ECU executed the request.
Strict mode rejects a positive reply to a suppressed request unless NRC 0x78
preceded it (`ProtocolDeviation::UnexpectedPositiveResponse`, ISO 14229-1:2013,
§8.2.2). `.strict(false)` accepts such replies. Expected silence is valid in
both modes; mandatory records are validated only when a response is received.

Negative responses remain `uds::Error::NegativeResponse` errors, including on
the suppressed view. After Response Pending (NRC 0x78), a final response is
mandatory regardless of suppression; missing it returns `Error::Timeout`
(ISO 14229-1:2013, Annex A.1). Transport errors and overall request deadlines
remain errors. Bit 7 can also be set directly in the sub-function passed to
`request()`, `diagnostic_session_control()`, `ecu_reset()`, or `security_access()`.

DiagnosticSessionControl adopts server-reported P2/P2* timings by default;
`.use_server_timing(false)` keeps the configured values. P2 uses 1 ms units
and P2* uses 10 ms units (ISO 14229-1:2013, §9.2.3.2, Tables 28–29).
In non-strict mode, a response without a timing record leaves the current timings
in place.

The transport returns complete messages, so the waits include response
reception (P6/P6* in ISO 14229-2, §7.1–§7.2). Set timeouts to cover network
delays and response transmission as needed. UDS retries transport idle receive
timeouts on the same stream without resending the request or extending P2/P2*.
Transport transmission and frame-reassembly timeouts still fail the request;
`IsoTPConfig::timeout` need not exceed P2/P2*. Session keepalive remains
caller-managed through `tester_present()`.

Strict mode follows ISO 14229-1:2013. It validates service IDs, sub-function
echoes, identifiers, block counters, and response formats. Negative responses
contain exactly three bytes and echo the requested SID.
Service methods validate mandatory records, fixed response lengths, memory-read
byte counts, memory-write echoes, and download/upload length formats.

Validation failures return
`uds::Error::ProtocolDeviation { deviation, response }` inside
`automotive::Error::UDSError`. The deviation describes the expectation, and
`response` contains the complete received UDS payload, including its service
header. `.strict(false)` disables these checks. Minimum parsing lengths apply
in both modes. Unknown NRCs and DTC formats are preserved as `NonStandard` in
both modes. NRC `0x78` invokes pending-response handling. Vehicle-specific record
contents and lengths are defined by the ECU.

## CAN Adapters
The following CAN adapters are supported.

### Supported CAN adapters
 - SocketCAN (Linux only, enable with the `socketcan` feature)
 - comma.ai panda (all platforms using [rusb](https://crates.io/crates/rusb), enable with the `panda` feature)
 - PEAK PCAN-USB FD family (all platforms using [rusb](https://crates.io/crates/rusb), enable with the `peak` feature)
 - J2534 PassThru Devices (Windows only, enable with the `j2534` feature)
 - Vector Devices (Windows x64 only, enable with the `vector-xl` feature)

Adapter support is opt-in. The default feature set does not enable any hardware adapters.

### Known limitations / Notes
This library has some unique features that might expose (performance) issues in drivers you wouldn't otherwise notice, so check the list of known limitations below.

This library supports awaiting a sent frame and waiting for the ACK on the CAN bus. This requires receiving these ACKs from the adapter, and matching them to the appropriate sent frame. This requires some level of hardware support that is not offered by all adapters/drivers. If this is not supported by the driver, an ACK will be simulated as soon as the frame is transmitted, but this can cause issues if precise timing is needed.

 - SocketCAN Devices
   - SocketCAN drivers without `IFF_ECHO`: This class of SocketCAN drivers has no hardware support for notifying the driver when a frame was ACKed. This is instead emulated by the [Linux kernel](https://github.com/torvalds/linux/blob/master/net/can/af_can.c#L256). Due to transmitted frames immediately being received again this can cause the receive queue to fill up if more than 476 (default RX queue size on most systems) are transmitted in one go. To solve this we implement emulated ACKs ourself, instead of relying on the ACKs from the kernel.
   - PCAN-USB: The Peak CAN adapters have two drivers:
     - Kernel built-in driver (`peak_usb`). The driver exposes `IFF_ECHO` and reports transmit completion when the USB write completes. It has a rather small TX queue, which can be increased with `ifconfig can0 txqueuelen <size>`.
     - Out-of-tree driver (`pcan`) that can be [downloaded](https://www.peak-system.com/fileadmin/media/linux/index.htm) from Peak System's website. The out-of-tree driver is not recommended as it does  not implement `IFF_ECHO`.
   - neoVI/ValueCAN: Use of Intrepid Control System's devices is not recommended due to issues in their SocketCAN driver. If many frames are transmitted simultaneously it will cause the whole system/kernel to hang. [intrepid-socketcan-kernel-module#20](https://github.com/intrepidcs/intrepid-socketcan-kernel-module/issues/20) tracks this issue.
 - comma.ai panda
   - The panda does not retry frames that are not ACKed, and drops them instead. This can cause panics in some internal parts of the library when frames are dropped. [panda#1922](https://github.com/commaai/panda/issues/1922) tracks this issue.
 - PEAK PCAN-USB FD (raw USB, `peak` feature)
   - This driver talks to the device directly over USB instead of through SocketCAN. Opening fails if a kernel driver such as `peak_usb` owns the interface. Opt in to temporary driver detachment with `PeakConfig::builder().detach_kernel_driver(true)`; the driver is reattached when the adapter is released.
   - On Windows, this backend requires WinUSB (installable with [Zadig](https://zadig.akeo.ie/)). The standard PEAK driver/PCAN-Basic used by Qt/SavvyCAN is unsupported. Replacing it disables PCAN-View/PCAN-Basic access until the PEAK driver is restored.
   - To temporarily unload `peak_usb` or the out-of-tree `pcan` driver for testing, run `scripts/unload_peak_driver.sh`. Test raw USB in another terminal, then press Enter to restore the drivers. The script also restores them on Ctrl+C or ordinary errors, and only reloads drivers that were initially loaded.
   - On Linux the USB device node is owned by root by default. To use the adapter without running as root, install a udev rule granting access to your user, e.g. in `/etc/udev/rules.d/70-pcan-usb-fd.rules`:
     ```text
     # PEAK System PCAN-USB FD family
     SUBSYSTEM=="usb", ATTRS{idVendor}=="0c72", MODE="0666"
     ```
     Then reload the rules and replug the device: `sudo udevadm control --reload-rules && sudo udevadm trigger`.
 - J2534 PassThru Devices are supported through the `j2534` feature. The library provides `J2534Adapter` for raw CAN access, and `J2534NativeIsoTP` to offload ISO-TP framing, flow-control, and timing to the adapter. It is recommended to use `J2534Adapter` unless you have specific speed or latency requirements.
 - Vector Devices are supported through the Vector XL Driver Library, and support can be enabled using the `vector-xl` feature. Make sure to distribute `vxlapi64.dll` alongside your application.
   - Configuring bus timing requires a data bitrate (`.data_bitrate(...)`) alongside
     nominal timing, even when sending only Classical CAN or FD frames without
     BRS. Vector's CAN FD configuration API requires valid timing for both phases.
     Shared access (`VectorConfig::new(channel)`) retains existing bus timing and
     does not require supplying bitrates.


### Implementing a New Adapter
Implementing a new adapter is done by implementing the `CanAdapter` Trait. Hardware implementations can be blocking, as the [AsyncCanAdapter](https://docs.rs/automotive/latest/automotive/can/struct.AsyncCanAdapter.html) takes care of presenting an async interface to the user. The library makes some assumptions around sending/receiving frames. These assumption are also verified by the tests in `tests/adapter_tests.rs`.

 - The `send` function takes a `&mut VecDequeue` of frames. Frames to be sent are taken from the *front* of this queue. If there is no space in the hardware or driver buffer to send out all messages it's OK to return before the queue is fully empty. If an error occurs make sure to put the message back at the beginning of the queue and return.
 - The hardware or driver is free to prioritize sending frames with a lower Arbitration ID to prevent priority inversion. However frames with the same Arbitration ID need to be send out on the CAN bus in the same order as they were queued. This assumption is needed to match a received ACK to the correct frame.
 - Once a frame is ACKed it should be put in the receive queue with the `loopback` flag set. The `AsyncCanAdapter` wrapper will take care of matching it against the right transmit frame and resolving the Future. If this is not supported by the underlying hardware, this can be faked by looping back all transmitted frames immediately.



 ## Roadmap
 Features I'd like to add in the future. Also check the [issues page](https://github.com/I-CAN-hack/automotive/issues?q=is%3Aopen+is%3Aissue+label%3Aenhancement).
 - CCP Client
 - Update file extraction (e.g. .frf and .odx)
 - VIN Decoding
 - More device support
 - WebUSB support
 - Python bindings
