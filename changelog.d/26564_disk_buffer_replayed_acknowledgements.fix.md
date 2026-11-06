Fixed the disk buffer acknowledging records before delivery after a restart. When Vector started with already-acknowledged records left in the buffer's current data file, the records it replayed to find its position were counted as acknowledgements and credited to the next records read. A later crash (for example `SIGKILL` or an out-of-memory kill) then skipped records that had been read but not delivered.

authors: omerfarukalkilic
