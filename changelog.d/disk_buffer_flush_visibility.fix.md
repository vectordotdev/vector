Fix disk-buffered events occasionally stalling until another write or a restart by waiting for asynchronous file writes to become visible before notifying the reader. Preserve accepted write progress across cancelled visibility flushes so resumed writes do not duplicate records. Both buffered and large direct writes are covered.

authors: phihos
