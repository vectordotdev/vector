Vector's TLS implementation now stores credentials in PEM format internally instead of PKCS12, enabling FIPS-compliant operation in
environments with strict cryptographic requirements. This change is transparent to users - both PEM and PKCS12 certificate files continue to
be supported as configuration inputs, with PKCS12 files automatically converted at load time.

authors: rf-ben
