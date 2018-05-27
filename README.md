journalDriver
=============

This is a small daemon used to forward logs from `journald` (systemd's
logging service) to [Stackdriver Logging][].

Most existing log services are written in inefficient dynamic
languages with error-prone "cover every use-case" configuration. This
tool aims to fit a specific use-case and fit it very well, instead of
covering every possible logging setup.

More documentation is forthcoming.

[Stackdriver Logging]: https://cloud.google.com/logging/
