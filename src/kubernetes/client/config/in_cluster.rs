//! Everything related to building in-cluster configuration.

use super::Config;
use crate::tls::TlsOptions;
use http::Uri;
use snafu::{ResultExt, Snafu};

impl Config {
    /// Prepares a config suitable for use when running in k8s cluster.
    pub fn in_cluster() -> Result<Self, Error> {
        let host = std::env::var("KUBERNETES_SERVICE_HOST").context(NotInCluster)?;
        let port = std::env::var("KUBERNETES_SERVICE_PORT").context(NotInCluster)?;

        let base = Uri::builder()
            .scheme("https")
            .authority(join_host_port(host.as_str(), port.as_str()).as_str())
            .path_and_query("/")
            .build()
            .context(InvalidUrl)?;

        let token_file = "/var/run/secrets/kubernetes.io/serviceaccount/token";
        let root_ca_file = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";

        let token = std::fs::read_to_string(token_file).context(Token)?;

        let mut tls_options = TlsOptions::default();
        tls_options.ca_file = Some(root_ca_file.into());

        Ok(Self {
            base,
            token,
            tls_options,
        })
    }
}

/// An error returned when building an in-cluster configuration.
#[derive(Debug, Snafu)]
pub enum Error {
    /// The in-cluster configuration requested while executing not in a cluster
    /// environment.
    #[snafu(display("unable to load in-cluster configuration, KUBERNETES_SERVICE_HOST and KUBERNETES_SERVICE_PORT must be defined"))]
    NotInCluster {
        /// The underlying error.
        source: std::env::VarError,
    },

    /// The token file could not be read successfully.
    #[snafu(display("unable to read the token file"))]
    Token {
        /// The underlying error.
        source: std::io::Error,
    },

    /// The configuration resulted in an invalid URL.
    #[snafu(display("unable to construct a proper API server URL"))]
    InvalidUrl {
        /// The underlying error.
        source: http::Error,
    },
}

fn join_host_port(host: &str, port: &str) -> String {
    if host.contains(":") {
        return format!("[{}]:{}", host, port);
    }
    format!("{}:{}", host, port)
}
