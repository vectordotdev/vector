Add systemd notify integration. Vector now sends `READY=1` when fully started, `STOPPING=1`
when beginning a graceful shutdown, and `WATCHDOG=1` pings at half the configured `WatchdogSec`
interval. The bundled `vector.service` and `hardened-vector.service` unit files are updated
to use `Type=notify`, with an optional `WatchdogSec` directive.
