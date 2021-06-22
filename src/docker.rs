use bollard::{errors::Error as DockerError, Docker, API_DEFAULT_VERSION};
use http::uri::Uri;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{env, path::PathBuf};

// From bollard source.
const DEFAULT_TIMEOUT: u64 = 120;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("URL has no host."))]
    NoHost,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DockerTlsConfig {
    ca_file: PathBuf,
    crt_file: PathBuf,
    key_file: PathBuf,
}

pub fn docker(host: Option<String>, tls: Option<DockerTlsConfig>) -> crate::Result<Docker> {
    let host = host.or_else(|| env::var("DOCKER_HOST").ok());

    match host {
        None => Docker::connect_with_local_defaults().map_err(Into::into),
        Some(host) => {
            let scheme = host
                .parse::<Uri>()
                .ok()
                .and_then(|uri| uri.into_parts().scheme);

            match scheme.as_ref().map(|scheme| scheme.as_str()) {
                Some("http") => {
                    let host = get_authority(&host)?;
                    Docker::connect_with_http(&host, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
                        .map_err(Into::into)
                }
                Some("https") => {
                    let host = get_authority(&host)?;
                    let tls = tls
                        .or_else(default_certs)
                        .ok_or(DockerError::NoCertPathError)?;
                    Docker::connect_with_ssl(
                        &host,
                        &tls.key_file,
                        &tls.crt_file,
                        &tls.ca_file,
                        DEFAULT_TIMEOUT,
                        API_DEFAULT_VERSION,
                    )
                    .map_err(Into::into)
                }
                Some("unix") | Some("npipe") | None => {
                    // TODO: Use `connect_with_local` on all platforms.
                    //
                    // Named pipes are currently disabled in Tokio. Tracking issue:
                    // https://github.com/fussybeaver/bollard/pull/138
                    if cfg!(windows) {
                        warn!("Named pipes are currently not available on Windows, trying to connecting to Docker with default HTTP settings instead.");
                        Docker::connect_with_http_defaults().map_err(Into::into)
                    } else {
                        Docker::connect_with_local(&host, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
                            .map_err(Into::into)
                    }
                }
                Some(scheme) => Err(format!("Unknown scheme: {}", scheme).into()),
            }
        }
    }
}

// From bollard source, unfortunately they don't export this function.
fn default_certs() -> Option<DockerTlsConfig> {
    let from_env = env::var("DOCKER_CERT_PATH").or_else(|_| env::var("DOCKER_CONFIG"));
    let base = match from_env {
        Ok(path) => PathBuf::from(path),
        Err(_) => dirs_next::home_dir()?.join(".docker"),
    };
    Some(DockerTlsConfig {
        ca_file: base.join("ca.pem"),
        key_file: base.join("key.pem"),
        crt_file: base.join("cert.pem"),
    })
}

fn get_authority(url: &str) -> Result<String, Error> {
    url.parse::<Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(<_>::to_string))
        .ok_or(Error::NoHost)
}
