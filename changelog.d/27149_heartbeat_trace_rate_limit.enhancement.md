The trace-level heartbeat message (`Beep.`) is now emitted at most once per minute instead of engaging the default 10-second internal log rate limiter, which cut its trace-mode log footprint ~4x while keeping the liveness marker visible.

authors: yackushevas
