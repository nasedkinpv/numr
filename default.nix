{
  pkgs ? import <nixpkgs> { },
}:
let
  manifest = (pkgs.lib.importTOML ./Cargo.toml).workspace.package;
in
pkgs.rustPlatform.buildRustPackage {
  pname = "numr";
  inherit (manifest) version;
  homepage = manifest.repository;

  cargoLock.lockFile = ./Cargo.lock;
  src = pkgs.lib.cleanSource ./.;

  buildInputs = [ pkgs.openssl ];
  nativeBuildInputs = [
    pkgs.rust-bin.stable.latest.default
    pkgs.pkg-config
  ];
}
