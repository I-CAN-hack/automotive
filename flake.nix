{
  description = "Development environment for the automotive Rust crate";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
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
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                cargo
                rustc
                rustfmt
                clippy
                rust-analyzer
                pkg-config
                clang
                git
                gnumake
                maturin
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
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        }
      );
    };
}
