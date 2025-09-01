{
  description = "launcher library for BYO-shells like ags and quickshell.";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };
  outputs = {
    self,
    nixpkgs,
  }: let
    supportedSystems = ["x86_64-linux"];
    forAllSystems = cb: nixpkgs.lib.genAttrs supportedSystems (system: cb pkgsFor.${system});
    pkgsFor = nixpkgs.legacyPackages;
  in {
    packages = forAllSystems (pkgs: {
      default = pkgs.callPackage ./. {};
    });

    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        name = "superspace-shell";
        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs = [pkgs.glib];
      };
    });
  };
}
