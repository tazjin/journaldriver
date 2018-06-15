journaldriver
=============

This is a small daemon used to forward logs from `journald` (systemd's
logging service) to [Stackdriver Logging][].

Most existing log services are written in inefficient dynamic
languages with error-prone "cover every use-case" configuration. This
tool aims to fit a specific use-case very well, instead of covering
every possible logging setup.

In the initial version `journaldriver` will only work if deployed
directly to a Google Compute Engine instance and will use the
[metadata server][] to figure out credentials and instance
identification.

## Usage

1. Install `journaldriver` on the instance from which you wish to
   forward logs.

2. Ensure that the instance has the appropriate permissions to write
   to Stackdriver. Google continously changes how IAM is implemented
   on GCP, so you will have to refer to [Google's documentation][].

   By default instances have the required permissions if Stackdriver
   Logging support is enabled in the project.

3. Start Stackdriver, for example via `systemd`.

## Upcoming features:

* `journaldriver` will be added to [nixpkgs][] with a complementary
  [NixOS][] module for easy configuration.
* `journaldriver` will persist the latest `journald` cursor position,
  allowing log reads to resume from the same position where they
  stopped after a restart
* `journaldriver` will attempt to figure out whether logs are in
  JSON-format and use the coresponding `jsonPayload` field in
  Stackdriver Logging to make structured logs easily accessible
* `journaldriver` will support deployments on non-GCP machines

[Stackdriver Logging]: https://cloud.google.com/logging/
[metadata server]: https://cloud.google.com/compute/docs/storing-retrieving-metadata
[Google's documentation]: https://cloud.google.com/logging/docs/access-control
[nixpkgs]: https://github.com/NixOS/nixpkgs/
[NixOS]: https://nixos.org/
