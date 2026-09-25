Added the `file_v2` source, intended as a modern replacement for the existing `file` source.
It uses async I/O and filesystem notifications to discover and read files, and introduces
a `checkpoint_interval` option to control checkpoint persistence.

authors: pront tamer-hassan thomasqueirozb
