# Adapters

Use `CanAdapter()` for discovery, or choose a backend explicitly. Constructors open
hardware immediately and block until ready. Sending and receiving are async.
Hardware is released when its last owner is dropped; `aclose()` and `async with`
remain available for deterministic cleanup.

```{eval-rst}
.. automodule:: automotive.adapters
   :members:
   :inherited-members:
   :undoc-members:
```
