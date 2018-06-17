# Nix derivation to build a release version of journaldriver.
#
# Note: This does not currently use Carnix due to an issue with
# linking against the `systemd.dev` derivation for libsystemd.

{ pkgs ? import <nixpkgs> {}
, doCheck ? true }:

with pkgs; rustPlatform.buildRustPackage {
  inherit doCheck;

  name        = "journaldriver";
  version     = "0.1.0";
  cargoSha256 = "0pf9dmras8lrqpz9ymax14a77g9w7w1x9bxz5mm159fzkhb4wz6d";

  src = ./.;

  buildInputs = [ pkgconfig openssl systemd.dev ];
}
