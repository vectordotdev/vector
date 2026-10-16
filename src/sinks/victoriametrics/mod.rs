//! The VictoriaMetrics sink.
//!
//! Delivers metrics using the VictoriaMetrics remote write protocol: a Prometheus `WriteRequest`
//! protobuf compressed with zstd. Distributions are encoded as VictoriaMetrics `vmrange`
//! histograms. Supports single-node VictoriaMetrics, cluster `vminsert` (tenant in the URL path or
//! in labels), and `vmauth` (tenant chosen by the credential).

use std::sync::Arc;

mod config;
mod encoder;
mod histogram;
mod normalize;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "victoriametrics-integration-tests"))]
mod integration_tests;

pub use config::VictoriaMetricsConfig;

/// The event secret holding a per-event bearer token.
///
/// When set, the token is sent as `Authorization: Bearer <token>` instead of the configured `auth`.
pub const TOKEN_SECRET_KEY: &str = "victoriametrics_token";

/// A VictoriaMetrics cluster tenant: `accountID` or `accountID:projectID`, both 32-bit unsigned
/// integers. The project defaults to `0`, as in VictoriaMetrics.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Tenant {
    account_id: u32,
    project_id: u32,
    // Normalized decimal forms, written as label values without per-series formatting.
    account: String,
    project: String,
}

impl Tenant {
    /// Parses a tenant like `auth.Token.Init` in VictoriaMetrics, but rejects signs and
    /// whitespace, which never appear in a valid tenant.
    pub(super) fn parse(id: &str) -> Option<Self> {
        let parse_part = |part: &str| {
            part.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| part.parse::<u32>().ok())
                .flatten()
        };

        let (account_id, project_id) = match id.split_once(':') {
            Some((account, project)) => (parse_part(account)?, parse_part(project)?),
            None => (parse_part(id)?, 0),
        };
        Some(Self {
            account_id,
            project_id,
            account: account_id.to_string(),
            project: project_id.to_string(),
        })
    }

    /// The URL path form, `accountID:projectID`.
    pub(super) fn path(&self) -> String {
        format!("{}:{}", self.account, self.project)
    }

    pub(super) const fn account_id(&self) -> u32 {
        self.account_id
    }

    pub(super) const fn project_id(&self) -> u32 {
        self.project_id
    }

    pub(super) fn account_label(&self) -> &str {
        &self.account
    }

    pub(super) fn project_label(&self) -> &str {
        &self.project
    }
}

/// Batches are split by the URL path tenant and by the per-event token, since both change the
/// request.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct PartitionKey {
    tenant: Option<Tenant>,
    token: Option<Arc<str>>,
}

#[cfg(test)]
mod tenant_tests {
    use super::Tenant;

    #[test]
    fn parses_valid_tenants() {
        let tenant = Tenant::parse("42").unwrap();
        assert_eq!((tenant.account_id(), tenant.project_id()), (42, 0));
        assert_eq!(tenant.path(), "42:0");

        let tenant = Tenant::parse("42:7").unwrap();
        assert_eq!(
            (tenant.account_label(), tenant.project_label()),
            ("42", "7")
        );
        assert_eq!(tenant.path(), "42:7");

        // Leading zeros are normalized, as `strconv.FormatUint` does in vmagent.
        assert_eq!(Tenant::parse("042:007").unwrap().path(), "42:7");
        assert_eq!(Tenant::parse("042:007"), Tenant::parse("42:7"));

        assert!(Tenant::parse("4294967295:0").is_some());
    }

    #[test]
    fn rejects_invalid_tenants() {
        for id in [
            "",
            ":",
            "42:",
            ":7",
            "a",
            "42:b",
            "-1",
            "+1",
            "1:2:3",
            "4294967296",
            "42/../x",
            " 42",
        ] {
            assert!(Tenant::parse(id).is_none(), "{id:?} must be rejected");
        }
    }
}
