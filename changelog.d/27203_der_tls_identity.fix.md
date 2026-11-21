Fixed loading DER-encoded X.509 certificates in `tls.crt_file` with a separate
`tls.key_file` for components using the shared TLS settings. PEM certificates and
PKCS#12 identities remain supported.

authors: titaneric
