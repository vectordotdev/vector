The default `/etc/vector/vector.yaml` config file is no longer installed by the Debian, RPM, Alpine, and distroless-static Docker packages. The previous default ran a `demo_logs` source and printed synthesized syslog lines to stdout, which then surfaced in journald or `/var/log/` on hosts running Vector as a service and was a common source of confusion.

New installs will now have no active config on disk. Provide your own configuration at `/etc/vector/vector.yaml` (or pass `--config <path>`) before starting Vector. A reference example is shipped at `/usr/share/vector/examples/vector.yaml`, and more sample configs remain at `/etc/vector/examples/`.

Existing installs are unaffected on upgrade: package managers preserve the on-disk `/etc/vector/vector.yaml` if you already had one.

authors: pront
