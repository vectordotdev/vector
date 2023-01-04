use std::io::Error;

fn main() -> Result<(), Error> {
    tonic_build::configure().build_server(true).compile(
        &[
            "proto/envoy/annotations/deprecation.proto",
            "proto/envoy/config/core/v3/address.proto",
            "proto/envoy/api/v2/core/address.proto",
            "proto/envoy/api/v2/core/backoff.proto",
            "proto/envoy/api/v2/core/base.proto",
            "proto/envoy/api/v2/core/http_uri.proto",
            "proto/envoy/api/v2/core/socket_option.proto",
            "proto/envoy/config/core/v3/backoff.proto",
            "proto/envoy/config/core/v3/base.proto",
            "proto/envoy/config/core/v3/http_uri.proto",
            "proto/envoy/data/accesslog/v3/accesslog.proto",
            "proto/envoy/service/accesslog/v2/als.proto",
            "proto/envoy/service/accesslog/v3/als.proto",
            "proto/envoy/type/percent.proto",
            "proto/envoy/type/semantic_version.proto",
            "proto/envoy/type/v3/percent.proto",
            "proto/envoy/type/v3/semantic_version.proto",
            "proto/udpa/annotations/migrate.proto",
            "proto/udpa/annotations/status.proto",
            "proto/udpa/annotations/versioning.proto",
            "proto/validate/validate.proto",
            "proto/xds/annotations/v3/status.proto",
            "proto/xds/core/v3/context_params.proto",
        ],
        &["proto", "../../proto"],
    )?;

    Ok(())
}
