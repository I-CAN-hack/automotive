# Bash Commands
 - `cargo test`: Run basic tests.
 - `cargo test --features=test-vcan`: Run tests against the virtual SocketCAN ECU.
   - If this fails, ensure the vcan interface is up: `scripts/set_up_vcan.sh`

# Workflow
 - Make sure to run all tests after making a series of code changes.
 - Keep `README.md` and the crate-level documentation in `src/lib.rs` in sync.

# Testing
 - Only add tests for protocol-level behavior.
 - Do not add tests for behavior already clear from the code or enforced by types.
