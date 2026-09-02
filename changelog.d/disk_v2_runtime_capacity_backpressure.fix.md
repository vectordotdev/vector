`disk_v2` buffers now apply their configured `when_full` policy when runtime filesystem space or quota is exhausted. Blocking buffers retry with backpressure, while drop-newest and overflow buffers promptly handle subsequent unwritten events according to policy. Records whose writes have already started remain owned by the disk buffer and complete exactly once after capacity recovers. Startup and non-capacity I/O failures remain fatal.

authors: Jansen-w
