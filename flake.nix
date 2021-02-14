{
  description = "a rust implementation of the 'crulz' macro language interpreter";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-20.09";
    flake-utils.url = "github:numtide/flake-utils";
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem (flake-utils.lib.allSystems)
      (system:
        rec {
          packages.crulz = (nixpkgs.callPackage ./Cargo.nix {}).rootCrate.build;
          defaultPackage = packages.crulz;
          apps.crulz = flake-utils.lib.mkApp { drv = packages.crulz; };
          defaultApp = apps.crulz;
          overlay = selfx: superx: {
            crulz = packages.crulz;
          };
        }
      );
}
