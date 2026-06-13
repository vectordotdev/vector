The `aws_s3` sink now picks a working default for `force_path_style` when `use_fips_endpoint = true`. Previously, `force_path_style` defaulted to `true`, which produced requests against the bare `s3-fips.<region>.amazonaws.com` hostname — but per [AWS][aws-fips], **all** S3 FIPS endpoints (commercial and GovCloud) only support virtual-hosted-style addressing. The result was every FIPS request failing with a dispatch error.

The new behavior:

- Unset `force_path_style` + `use_fips_endpoint = true` → defaults to `false` (virtual-hosted-style), the only addressing mode AWS supports for S3 FIPS endpoints.
- Unset `force_path_style` everywhere else → still defaults to `true` (no behavior change).
- Explicit `force_path_style = true` together with `use_fips_endpoint = true` → Vector overrides the value back to `false` and emits a warning at startup. The two settings are unsupported in combination by AWS, regardless of region.
- Explicit `force_path_style = false` → honored as `false`.

[aws-fips]: https://aws.amazon.com/compliance/fips/

authors: joshcoughlan
