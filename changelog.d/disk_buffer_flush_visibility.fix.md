Fix disk-buffered events occasionally stalling until another write or a restart by waiting for asynchronous file writes to become visible before notifying the reader. Both buffered and large direct writes are covered.

authors: phihos
