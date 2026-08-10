Prevent `disk_v2` buffers from failing when the filesystem runs out of space or quota at runtime. Writers now apply backpressure and retry until storage becomes available, while capacity errors during startup remain fatal.

authors: Jansen-w
