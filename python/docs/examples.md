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

Exchange complete messages through a software ISO-TP transport.

```{literalinclude} ../examples/isotp.py
:language: python
```

## Query an ECU with UDS

Read diagnostic data through a persistent UDS client.

```{literalinclude} ../examples/uds.py
:language: python
```

## Configure bitrate and sample points

Reuse adapter configuration with explicit nominal and data-phase timing.

```{literalinclude} ../examples/configure_bitrate.py
:language: python
```

## Query firmware versions concurrently

Use independent ECU transports for concurrent diagnostic requests.

```{literalinclude} ../examples/query_fw_versions.py
:language: python
```

## Synchronous examples

These programs use the same configuration and values through `automotive.sync`.
They use ordinary constructors, blocking calls, and receive iterators. Both the
async and sync examples let ownership release resources when they go out of scope.

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
