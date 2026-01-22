Fixed an issue where `ProfileFileCredentialsProvider` cached AWS credentials indefinitely, causing `ExpiredToken` errors in long-running processes with externally rotated credentials (IRSA, aws-vault, saml2aws). Added `RefreshingFileCredentialsProvider` that periodically re-reads credentials from file with a configurable `refresh_interval_secs` option (default: 5 minutes).

authors: TayPark
