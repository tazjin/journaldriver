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

## Features

* `journaldriver` persists the last forwarded position in the journal
  and will resume forwarding at the same position after a restart
* `journaldriver` will recognise log entries in JSON format and
  forward them appropriately to make structured log entries available
  in Stackdriver
* `journaldriver` can be used outside of GCP by configuring static
  credentials

## Usage on Google Cloud Platform

`journaldriver` does not require any configuration when running on GCP
instances.

1. Install `journaldriver` on the instance from which you wish to
   forward logs.

2. Ensure that the instance has the appropriate permissions to write
   to Stackdriver. Google continously changes how IAM is implemented
   on GCP, so you will have to refer to [Google's documentation][].

   By default instances have the required permissions if Stackdriver
   Logging support is enabled in the project.

3. Start `journaldriver`, for example via `systemd`.


## Usage outside of Google Cloud Platform

When running outside of GCP, the following extra steps need to be
performed:

1. Create a Google Cloud Platform service account with the "Log
   Writer" role and download its private key in JSON-format.
2. When starting `journaldriver`, configure the following environment
   variables:

   * `GOOGLE_CLOUD_PROJECT`: Name of the GCP project to which logs
     should be written.
   * `GOOGLE_APPLICATION_CREDENTIALS`: Filesystem path to the
     JSON-file containing the service account's private key.
   * `LOG_NAME`: Name of the target log stream in Stackdriver Logging.
     This will be automatically created if it does not yet exist.

## Upcoming features:

* `journaldriver` will be added to [nixpkgs][] with a complementary
  [NixOS][] module for easy configuration.

[Stackdriver Logging]: https://cloud.google.com/logging/
[metadata server]: https://cloud.google.com/compute/docs/storing-retrieving-metadata
[Google's documentation]: https://cloud.google.com/logging/docs/access-control
[nixpkgs]: https://github.com/NixOS/nixpkgs/
[NixOS]: https://nixos.org/
