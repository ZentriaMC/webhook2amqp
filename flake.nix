{
  description = "webhook2amqp";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";

    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    let
      # https://rust-lang.github.io/rustup-components-history/
      rustVersion = "1.64.0";
      supportedSystems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];
    in
    flake-utils.lib.eachSystem supportedSystems (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rust = pkgs.rust-bin.stable."${rustVersion}".minimal.override {
          extensions = [ "clippy" "rust-src" "rustfmt" "rust-analyzer" ];
        };
      in
      rec {
        devShell = pkgs.mkShell {
          buildInputs = [
            rust
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
            Security
          ]);
        };
      });
}
