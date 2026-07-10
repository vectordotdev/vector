Fixed a bug in the `file` source where checkpoints recording the last-read file position were not always fully written before Vector shut down. On the next startup, the `file` source could start reading from an earlier position, causing events to be re-processed.

authors: thomasqueirozb
