# Bash Commands
 - `cargo test`: Run basic tests.
 - `cargo test --features=test-vcan`: Run tests against the virtual SocketCAN ECU.
   - If this fails, ensure the vcan interface is up: `scripts/set_up_vcan.sh`

# Workflow
 - Make sure to run all tests after making a series of code changes.
