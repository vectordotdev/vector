Changes the fingerprint for file sources to use uncompressed file content
as a source of truth when fingerprinting lines and checking
`ignored_header_bytes`. Previously this was using the compressed bytes. For now, only gzip compression is supported.

authors: roykim98
