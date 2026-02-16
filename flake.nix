{
  description = "Matrix bot development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Latest stable Rust toolchain with essential components
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src" # For rust-analyzer
            "rust-analyzer" # LSP server
            "clippy" # Linter
            "rustfmt" # Formatter
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain

            # Native dependencies for matrix-sdk
            pkgs.pkg-config
            pkgs.openssl
            pkgs.openssl.dev
            pkgs.sqlite
          ];

          # Environment variables for building
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
          RUST_BACKTRACE = 1;

          # Fix for rust-analyzer: use standard /tmp instead of nix-shell-specific TMPDIR
          # The nix-shell TMPDIR gets cleaned up, causing proc-macro server to crash
          TMPDIR = "/tmp";

          shellHook = ''
            echo "Rust development environment loaded!"
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
          '';
        };
      }
    );
}
