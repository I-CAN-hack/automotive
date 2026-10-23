{
  description = "Development environment for the automotive Rust crate";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
    in
    {
      devShells = nixpkgs.lib.genAttrs systems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rustToolchain = pkgs.rust-bin.stable.latest.minimal.override {
            targets = [
              "x86_64-unknown-linux-gnu"
              "aarch64-unknown-linux-gnu"
              "armv7-unknown-linux-gnueabihf"
              "aarch64-apple-darwin"
              "x86_64-pc-windows-msvc"
            ];
            extensions = [
              "clippy"
              "rust-analyzer"
              "rust-src"
              "rustfmt"
            ];
          };
          # Fetch the SDK using nixpkgs' pinned Apple URLs/checksum. The raw SDK
          # is platform-independent, so Linux can use it with Zig's Darwin linker.
          sdkVersions = builtins.fromJSON (
            builtins.readFile "${nixpkgs}/pkgs/by-name/ap/apple-sdk/metadata/versions.json"
          );
          macosSdk =
            pkgs.callPackage "${nixpkgs}/pkgs/by-name/ap/apple-sdk/common/fetch-sdk.nix" { }
              sdkVersions."14";
        in
        {
          default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                rustToolchain
                pkg-config
                llvmPackages.clang-unwrapped
                llvmPackages.bintools-unwrapped
                git
                gnumake
                maturin
                uv
                (python3.withPackages (pythonPackages: [
                  pythonPackages.scapy
                  pythonPackages.pytest
                  pythonPackages.pytest-asyncio
                  pythonPackages.pip
                ]))
              ]
              ++ lib.optionals stdenv.hostPlatform.isLinux [
                can-utils
                iproute2
                kmod
              ];

            buildInputs = [ pkgs.libusb1 ];

            LIBCLANG_PATH = "${pkgs.lib.getLib pkgs.llvmPackages.libclang}/lib";
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            SDKROOT = macosSdk;
          };
        }
      );
    };
}
