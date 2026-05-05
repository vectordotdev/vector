The `elasticsearch` and `prometheus_remote_write` sinks no longer expose `aws.use_fips_endpoint` or `aws.endpoint` in their configuration schema. Both components POST to a user-supplied URL and use the AWS SDK only to sign requests with SigV4 — they never let the SDK resolve an endpoint, so neither field has any effect on those paths. Previously, both fields were inherited from the shared `RegionOrEndpoint` type and silently accepted, which was misleading for a compliance-sensitive flag like `use_fips_endpoint`.

The two sinks now use a smaller `AwsAuthRegion` type that exposes only `aws.region`. Configurations that only set `aws.region` are unaffected. Configurations that set `aws.endpoint` or `aws.use_fips_endpoint` on these sinks will now fail to load with an unknown-field error — they were never honored, so this surfaces what was already broken.

To use a FIPS endpoint with these sinks, point `endpoint`/`endpoints` at the FIPS hostname directly.

authors: joshcoughlan
