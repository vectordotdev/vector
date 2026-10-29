The `distroless-libc` Docker image now uses `gcr.io/distroless/cc-debian13` as its base instead of `cc-debian12`. The Debian 12 CA bundle still trusts the `emSign ECC Root CA - G3` and `AAA Certificate Services` roots that the eMudhra intermediate certificate compromise affects (CVE-2026-111943). The Debian 13 bundle removes them.

authors: thomasqueirozb
