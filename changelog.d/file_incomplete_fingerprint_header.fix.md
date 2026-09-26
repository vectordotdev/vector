Fixed the `file` source hanging during fingerprinting when a file is smaller than `fingerprint.ignored_header_bytes`. Incomplete files are retried when more data becomes available.

authors: pront
