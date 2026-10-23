# CAN

`automotive.can` provides frames and identifiers, with bus timing in
`automotive.can.bitrate`. The same types are available through
`automotive.sync.can` and `automotive.sync.can.bitrate`.

```{toctree}
:maxdepth: 1

bitrate
```

## Frames and identifiers

In both APIs, identifiers compare directly with numbers using their raw value:
`frame.id == rx_addr` works when `rx_addr` is an integer. Ordering comparisons
also support range checks such as `0x600 <= frame.id < 0x800`. Error frames have
no identifier; check `frame.id is not None` before ordering those frames.
Equality between two `Identifier` objects also requires the same standard or
extended format.

```{eval-rst}
.. automodule:: automotive.can
   :members: Frame, Identifier, ErrorFlag
   :undoc-members:
```

