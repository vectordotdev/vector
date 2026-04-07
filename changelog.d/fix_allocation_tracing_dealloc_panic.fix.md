Fixed a panic (abort) when running with `--allocation-tracing` in debug builds, caused by deallocating memory that was allocated before tracking was enabled. Also fixed per-group memory accounting skew for reentrant allocations whose tracing closure was skipped, which left the group ID header uninitialized and caused deallocations to be attributed to wrong groups.

authors: pront
