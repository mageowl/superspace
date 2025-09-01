{pkgs ? import <nixpkgs> {}}: let
  manifest = (pkgs.lib.importTOML ../Cargo.toml).package;
in
  pkgs.rustPlatform.buildRustPackage {
    pname = manifest.name;
    version = manifest.version;

    src = pkgs.lib.cleanSource ../.;
    cargoLock.lockFile = ../Cargo.lock;

    nativeBuildInputs = with pkgs; [pkg-config];
    buildInputs = with pkgs; [glib];
  }
