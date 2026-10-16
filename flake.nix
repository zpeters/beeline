{
  description = "beeline: a minimal API client GUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        # Rust/Cargo comes from rustup, not this shell. This just
        # provides the surrounding dev tooling (see justfile).
        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.just
          ];
        };
      }
    );
}
