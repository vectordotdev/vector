The `journald` source now correctly respects the `current_boot_only: false` setting on systemd versions >= 258.

Compatibility notes:
- **systemd < 250**: Both `current_boot_only: true` and `false` work correctly
- **systemd 250-257**: Due to systemd limitations, `current_boot_only: false` will not work and Vector will only collect logs from the current boot. A warning will be logged on startup.
- **systemd >= 258**: Both settings work correctly

authors: bachorp
