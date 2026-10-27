Docker images no longer trust the revoked eMudhra `emSign Root CA - C1`, `emSign ECC Root CA - C3`, and `emSign ECC Root CA - G3` root certificates (CVE-2026-111943). TLS connections to servers whose chains end at a revoked root now fail validation.

authors: andyqvo
