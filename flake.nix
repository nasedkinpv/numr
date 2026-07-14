{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];
    in
    nixpkgs.lib.genAttrs systems (
      system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          build = pkgs.callPackage ./. { };
        in
        {
          packages.default = build;
          devShells.default = pkgs.mkShellNoCC {
            inputsFrom = [ build ];
          };
        }
    );
}
