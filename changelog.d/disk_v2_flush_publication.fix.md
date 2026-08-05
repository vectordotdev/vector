Fixed a race in `disk_v2` buffers where a record could become visible to the reader before its
asynchronous file write and shared accounting state were fully published. This could cause a
corrupted read or leave the buffer stalled.

authors: graphcareful
