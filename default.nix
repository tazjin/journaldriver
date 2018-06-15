# Nix derivation to build a release version of journaldriver.
#
# Note: This does not currently use Carnix due to an issue with
# linking against the `systemd.dev` derivation for libsystemd.

{ pkgs ? import <nixpkgs> {}}:

with pkgs; rustPlatform.buildRustPackage {
  name        = "journaldriver";
  version     = "0.1.0";
  cargoSha256 = "05iwidi66f0lssbkgn13rnvlqmajdbdp859wv2a1xqvi8fcpqsmy";

  src = ./.;

  buildInputs = [ pkgconfig openssl systemd.dev ];
}
