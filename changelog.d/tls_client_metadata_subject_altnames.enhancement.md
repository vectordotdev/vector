The `tls_client_metadata` metadata field added by TCP-based sources with `client_metadata_key`
set now includes `subject_altnames`, containing the Subject Alternative Names (DNS names, email
addresses, URIs, and IP addresses) from the client TLS certificate. Each SAN is prefixed with its
type (for example `DNS:example.com`, `email:admin@example.com`, `URI:https://example.com`, or
`IP Address:127.0.0.1`) to match the output of `openssl x509 -text`. This key is only added when
the client certificate contains Subject Alternative Names.

authors: emillen
