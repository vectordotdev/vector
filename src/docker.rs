#![allow(missing_docs)]
use std::{collections::HashMap, env, path::PathBuf};

use bollard::{
    container::{Config, CreateContainerOptions},
    errors::Error as DockerError,
    image::{CreateImageOptions, ListImagesOptions},
    models::HostConfig,
    Docker, API_DEFAULT_VERSION,
};
use futures::StreamExt;
use http::uri::Uri;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;

// From bollard source.
const DEFAULT_TIMEOUT: u64 = 120;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("URL has no host."))]
    NoHost,
}

/// Configuration of TLS when connecting to the Docker daemon.
///
/// Only relevant when connecting to Docker with an HTTPS URL.
///
/// If not configured, the environment variable `DOCKER_CERT_PATH` is used. If `DOCKER_CERT_PATH` is absent, then` DOCKER_CONFIG` is used. If both environment variables are absent, the certificates in `~/.docker/` are read.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DockerTlsConfig {
    /// Path to the CA certificate file.
    ca_file: PathBuf,

    /// Path to the TLS certificate file.
    crt_file: PathBuf,

    /// Path to the TLS key file.
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
                Some("http") | Some("tcp") => {
                    let host = get_authority(&host)?;
                    Docker::connect_with_http(&host, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
                        .map_err(Into::into)
                }
                Some("https") => {
                    let host = get_authority(&host)?;
                    let tls = tls
                        .or_else(default_certs)
                        .ok_or(DockerError::NoHomePathError)?;
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
                    Docker::connect_with_local(&host, DEFAULT_TIMEOUT, API_DEFAULT_VERSION)
                        .map_err(Into::into)
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

async fn pull_image(docker: &Docker, image: &str, tag: &str) {
    let mut filters = HashMap::new();
    filters.insert(
        String::from("reference"),
        vec![format!("{}:{}", image, tag)],
    );

    let options = Some(ListImagesOptions {
        filters,
        ..Default::default()
    });

    let images = docker.list_images(options).await.unwrap();
    if images.is_empty() {
        // If not found, pull it
        let options = Some(CreateImageOptions {
            from_image: image,
            tag,
            ..Default::default()
        });

        docker
            .create_image(options, None, None)
            .for_each(|item| async move {
                let info = item.unwrap();
                if let Some(error) = info.error {
                    panic!("{:?}", error);
                }
            })
            .await
    }
}

async fn remove_container(docker: &Docker, id: &str) {
    trace!("Stopping container.");

    _ = docker
        .stop_container(id, None)
        .await
        .map_err(|e| error!(%e));

    trace!("Removing container.");

    // Don't panic, as this is unrelated to the test
    _ = docker
        .remove_container(id, None)
        .await
        .map_err(|e| error!(%e));
}

pub struct Container {
    image: &'static str,
    tag: &'static str,
    binds: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
}

impl Container {
    pub const fn new(image: &'static str, tag: &'static str) -> Self {
        Self {
            image,
            tag,
            binds: None,
            cmd: None,
        }
    }

    pub fn bind(mut self, src: impl std::fmt::Display, dst: &str) -> Self {
        let bind = format!("{}:{}", src, dst);
        self.binds.get_or_insert_with(Vec::new).push(bind);
        self
    }

    pub fn cmd(mut self, option: &str) -> Self {
        self.cmd.get_or_insert_with(Vec::new).push(option.into());
        self
    }

    pub async fn run<T>(self, doit: impl futures::Future<Output = T>) -> T {
        let docker = docker(None, None).unwrap();

        pull_image(&docker, self.image, self.tag).await;

        let options = Some(CreateContainerOptions {
            name: format!("vector_test_{}", uuid::Uuid::new_v4()),
            platform: None,
        });

        let config = Config {
            image: Some(format!("{}:{}", &self.image, &self.tag)),
            cmd: self.cmd,
            host_config: Some(HostConfig {
                network_mode: Some(String::from("host")),
                extra_hosts: Some(vec!["host.docker.internal:host-gateway".into()]),
                binds: self.binds,
                ..Default::default()
            }),
            ..Default::default()
        };

        let container = docker.create_container(options, config).await.unwrap();

        docker
            .start_container::<String>(&container.id, None)
            .await
            .unwrap();

        let result = doit.await;

        remove_container(&docker, &container.id).await;

        result
    }
}
