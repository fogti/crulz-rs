let
  sources = import ./sources.nix;
  rustChannelsOverlay = import "${sources.rust-overlay}/rust-overlay.nix";

in import sources.nixpkgs {
  overlays = [
    rustChannelsOverlay
    (self: super: {
      # Replace "rust-bin.stable.latest" with the version of the rust tools that
      # you would like. Look at the documentation of nixpkgs-mozilla for examples.
      #
      # NOTE: "rust" instead of "rustc" is not a typo: It will include more than needed
      # but also the much needed "rust-std".
      rustc = super.rust-bin.stable.latest.rust;
      inherit (super.rust-bin.stable.latest) cargo rust rust-fmt rust-std clippy;
    })
  ];
}
