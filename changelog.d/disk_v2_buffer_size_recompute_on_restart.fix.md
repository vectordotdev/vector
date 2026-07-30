After a crash, affected `disk_v2` buffers could incorrectly appear full, block new events, and stall recovery. Vector now restores buffer usage correctly on restart so the pipeline can continue processing.

authors: graphcareful
