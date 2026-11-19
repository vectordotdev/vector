Improve the `journald` startup error for `current_boot_only: false` with systemd
versions 250 through 257. The error identifies the `journalctl` executable and
explains how to use a compatible version, including in containers, or collect
only the current boot instead.

authors: pront
