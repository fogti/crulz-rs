{
  description = "a rust implementation of the 'crulz' macro language interpreter";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-20.09";
    yz-flake-utils.url = "github:YZITE/flake-utils";
    # needed for default.nix, shell.nix
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };
  outputs = { nixpkgs, yz-flake-utils, ... }:
    yz-flake-utils.lib.mkFlakeFromProg {
      prevpkgs = nixpkgs;
      progname = "crulz";
      drvBuilder = final: prev: (import ./Cargo.nix { pkgs = final; }).rootCrate.build;
    };
}
