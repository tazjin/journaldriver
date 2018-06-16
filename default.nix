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
  cargoSha256 = "165hmmsy9y7334g80yv21ma91rfavv2jk8fqssrccas7ryj4abki";

  src = ./.;

  buildInputs = [ pkgconfig openssl systemd.dev ];
}
