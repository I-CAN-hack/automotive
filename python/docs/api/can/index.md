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
`frame.id == rx_addr` works when `rx_addr` is an integer. Comparisons between
two `Identifier` objects also require the same standard or extended format.

```{eval-rst}
.. automodule:: automotive.can
   :members: Frame, Identifier, ErrorFlag
   :undoc-members:
```

