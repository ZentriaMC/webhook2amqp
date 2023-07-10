{
  description = "webhook2amqp";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";

    crane.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane, ... }:
    let
      # https://rust-lang.github.io/rustup-components-history/
      rustVersion = "1.69.0";
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

        craneLib = (crane.mkLib pkgs).overrideToolchain rust;
      in
      rec {
        packages.webhook2amqp = pkgs.callPackage
          ({ lib, stdenv, pkg-config, luajit, libiconv, Security }: craneLib.buildPackage {
            src = craneLib.cleanCargoSource (./.);

            buildInputs = [
              luajit
            ] ++ lib.optionals stdenv.isDarwin [
              libiconv
              Security
            ];

            nativeBuildInputs = [
              pkg-config
            ];

            doCheck = false; # no tests
          })
          {
            inherit (pkgs.darwin.apple_sdk.frameworks) Security;
          };

        devShell = pkgs.mkShell {
          buildInputs = [
            rust
            pkgs.luajit
            pkgs.luajitPackages.moonscript
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
            Security
          ]);

          nativeBuildInputs = [
            pkgs.pkg-config
          ];
        };
      });
}
