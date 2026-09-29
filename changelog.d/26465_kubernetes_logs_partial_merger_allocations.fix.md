Reduce the memory usage of the `kubernetes_logs` source for containers that log partial
lines (CRI `P` lines).

For every partial line the partial event merger copied the whole accumulated message into
a new buffer, which is quadratic in the length of a partial run, and it converted the
`file` field of every event into a new `String`. The accumulated message is now extended
in place, buckets are keyed by the bytes of the `file` field, and expired buckets are
moved instead of cloned.

On a synthetic all-partial backlog (20 files, 150k partial lines) this lowers the bytes
allocated by the source from 12.6 GB to 0.15 GB and steady memory usage from 263 MiB to
176 MiB, for the same merged events. The events emitted by the merger, including
truncated and oversized ones, are unchanged.

authors: thomasqueirozb
