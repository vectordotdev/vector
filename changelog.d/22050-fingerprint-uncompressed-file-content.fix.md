Changes the fingerprint for file sources to use uncompressed file content
as a source of truth when fingerprinting lines and checking
ignored_header_bytes. Previously this was using the compressed bytes. Only gzip
supported for now.

authors: roykim98
