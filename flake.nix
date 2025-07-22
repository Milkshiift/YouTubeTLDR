{
  description = "OpenSSL";

  # I already have a system-wide rust toolchain so it's not included here

  inputs = {
      nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
      flake-utils.url = "github:numtide/flake-utils";
    };

    outputs = { self, nixpkgs, flake-utils, ... }:
      flake-utils.lib.eachDefaultSystem (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          devShells.default = pkgs.mkShell {
            nativeBuildInputs = [
              pkgs.upx
              pkgs.openssl
              pkgs.pkg-config
            ];

            LD_LIBRARY_PATH = with pkgs; lib.makeLibraryPath [ openssl ];
          };
        });
}