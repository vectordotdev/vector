The `host_metrics` source now includes a `psi` collector for Linux Pressure Stall Information metrics. It reports `psi_avg10`, `psi_avg60`, `psi_avg300` (gauges) and `psi_total` (counter) for CPU, memory, I/O, and IRQ pressure with `resource` and `level` tags.

authors: mono
