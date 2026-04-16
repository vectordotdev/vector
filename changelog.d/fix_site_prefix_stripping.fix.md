Fixed the Datadog sink healthcheck endpoint computation to preserve site prefixes (e.g. `us3.`, `us5.`, `ap1.`) when deriving the API URL from intake endpoints. Previously, the healthcheck for site-specific endpoints like `https://http-intake.logs.us3.datadoghq.com` would incorrectly call `https://api.datadoghq.com` instead of `https://api.us3.datadoghq.com`, causing unintended cross-site egress traffic.

authors: vladimir-dd
