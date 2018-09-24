journaldriver
=============

This is a small daemon used to forward logs from `journald` (systemd's
logging service) to [Stackdriver Logging][].

Most existing log services are written in inefficient dynamic
languages with error-prone "cover every use-case" configuration. This
tool aims to fit a specific use-case very well, instead of covering
every possible logging setup.

`journaldriver` can be run on GCP-instances with no additional
configuration as authentication tokens are retrieved from the
[metadata server][].

## Features

* `journaldriver` persists the last forwarded position in the journal
  and will resume forwarding at the same position after a restart
* `journaldriver` will recognise log entries in JSON format and
  forward them appropriately to make structured log entries available
  in Stackdriver
* `journaldriver` can be used outside of GCP by configuring static
  credentials
* `journaldriver` will recognise journald's log priority levels and
  convert them into equivalent Stackdriver log severity levels

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
   * `LOG_STREAM`: Name of the target log stream in Stackdriver Logging.
     This will be automatically created if it does not yet exist.
   * `LOG_NAME`: Name of the target log to write to. This defaults to
     `journaldriver` if unset, but it is recommended to - for
     example - set it to the machine hostname.

## Log levels / severities / priorities

`journaldriver` recognises [journald's priorities][] and converts them
into [equivalent severities][] in Stackdriver. Both sets of values
correspond to standard `syslog` priorities.

The easiest way to emit log messages with priorites from an
application is to use [priority prefixes][], which are compatible with
structured log messages.

For example, to emit a simple warning message (structured and
unstructured):

```
$ echo '<4>{"fnord":true, "msg":"structured log (warning)"}' | systemd-cat
$ echo '<4>unstructured log (warning)' | systemd-cat
```

## NixOS module

At Aprila we deploy all of our software using [NixOS][], including
`journaldriver`. The NixOS package repository [contains a module][]
for setting up `journaldriver`.

On a GCP instance the only required option is this:

```nix
services.journaldriver.enable = true;
```

When running outside of GCP, the configuration looks as follows:

```nix
services.journaldriver = {
  enable                 = true;
  logStream              = "prod-environment";
  logName                = "hostname";
  googleCloudProject     = "gcp-project-name";
  applicationCredentials = keyFile;
};
```

**Note**: The `journaldriver`-module is not yet included in a stable
release of NixOS, but it is available on the `unstable`-channel.

[Stackdriver Logging]: https://cloud.google.com/logging/
[metadata server]: https://cloud.google.com/compute/docs/storing-retrieving-metadata
[Google's documentation]: https://cloud.google.com/logging/docs/access-control
[NixOS]: https://nixos.org/
[contains a module]: https://github.com/NixOS/nixpkgs/pull/42134
[journald's priorities]: http://0pointer.de/public/systemd-man/sd-daemon.html
[equivalent severities]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
[priority prefixes]: http://0pointer.de/public/systemd-man/sd-daemon.html
