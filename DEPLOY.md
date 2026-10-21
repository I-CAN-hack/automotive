# Rust deployment

Run these commands from the repository root.

Keep the Rust versions in `Cargo.toml` and `python/Cargo.toml` aligned when
preparing releases. The Python package version is set in `python/pyproject.toml`.

After the Rust tests pass, verify the package and publish it locally:

```sh
scripts/deploy_crates.sh --dry-run
cargo login
scripts/deploy_crates.sh
```

The script publishes only the `automotive` Rust crate to crates.io, using the
lockfile and verifying a build with all adapters enabled. Cargo requires a clean
working tree; use `--dry-run --allow-dirty` to inspect an uncommitted release.
`CARGO_REGISTRY_TOKEN` can be used instead of `cargo login`. See the
[Cargo publishing documentation](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).
Python builds and PyPI publishing are documented in the [Python deployment guide](python/DEPLOY.md).
