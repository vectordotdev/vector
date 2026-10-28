The `databricks_zerobus` sink now verifies server certificates and hostnames for OAuth and Unity Catalog schema requests. Previously, these requests disabled TLS verification, which could expose OAuth credentials and schema responses to a network attacker.

authors: thomasqueirozb
