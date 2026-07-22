Fixed a rare `disk_v2` disk buffer stall where a reader could wait indefinitely
after consuming a writer notification but failing to observe the corresponding
record during a concurrent read/write race. Reader waits for writer progress are
now bounded so the reader periodically re-checks on-disk state even if no further
writer notification arrives.

authors: jjh5887
