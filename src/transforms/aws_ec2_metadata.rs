use super::Transform;
use crate::{
    event::Event,
    runtime::TaskExecutor,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use bytes::Bytes;
use futures::Stream;
use futures03::compat::Future01CompatExt;
use http::{uri::PathAndQuery, header::HeaderName, Request, StatusCode, Uri};
use hyper::{client::connect::HttpConnector, Body, Client};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;
use tokio::timer::Delay;

lazy_static::lazy_static! {
    static ref AMI_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/ami-id");
    static ref AMI_ID_KEY: Atom = Atom::from("ami-id");

    static ref AVAILABILITY_ZONE: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/placement/availability-zone");
    static ref AVAILABILITY_ZONE_KEY: Atom = Atom::from("availability-zone");

    static ref INSTANCE_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/instance-id");
    static ref INSTANCE_ID_KEY: Atom = Atom::from("instance-id");

    static ref LOCAL_HOSTNAME: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/local-hostname");
    static ref LOCAL_HOSTNAME_KEY: Atom = Atom::from("local-hostname");

    static ref LOCAL_IPV4: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/local-ipv4");
    static ref LOCAL_IPV4_KEY: Atom = Atom::from("local-ipv4");

    static ref PUBLIC_HOSTNAME: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/public-hostname");
    static ref PUBLIC_HOSTNAME_KEY: Atom = Atom::from("public-hostname");

    static ref PUBLIC_IPV4: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/public-ipv4");
    static ref PUBLIC_IPV4_KEY: Atom = Atom::from("public-ipv4");

    static ref REGION: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/region");
    static ref REGION_KEY: Atom = Atom::from("region");

    static ref ROLE_NAME: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/iam/security-credentials/role-name");
    static ref ROLE_NAME_KEY: Atom = Atom::from("role-name");

    static ref SUBNET_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/network/interfaces/macs/mac/subnet-id");
    static ref SUBNET_ID_KEY: Atom = Atom::from("subnet-id");

    static ref VPC_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/network/interfaces/macs/mac/vpc-id");
    static ref VPC_ID_KEY: Atom = Atom::from("vpc-id");

    static ref DYNAMIC_DOCUMENT: PathAndQuery = PathAndQuery::from_static("/latest/dynamic/document");

    static ref API_TOKEN: PathAndQuery = PathAndQuery::from_static("/latest/api/token");
    static ref TOKEN_HEADER: HeaderName = HeaderName::from_static("");
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ec2Metadata {
    namespace: Option<String>,
    refresh_interval: Option<Duration>,
    fields: Option<Vec<String>>,
}

pub struct Ec2MetadataTransform {
    state: Arc<RwLock<Shared>>,

    // Key atoms that we can reuse
    ami_id_key: Atom,
    availability_zone_key: Atom,
    instance_id_key: Atom,
    local_hostname_key: Atom,
    local_ipv4_key: Atom,
    public_hostname_key: Atom,
    public_ipv4_key: Atom,
    region_key: Atom,
    // role_name_key: Atom,
    subnet_id_key: Atom,
    vpc_id_key: Atom,
}

#[derive(Clone, Debug)]
struct Shared {
    metadata: Option<InstanceMetadata>,
}

inventory::submit! {
    TransformDescription::new_without_default::<Ec2Metadata>("aws_ec2_metadata")
}

#[typetag::serde(name = "aws_ec2_metadata")]
impl TransformConfig for Ec2Metadata {
    fn build(&self, exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        let state = Arc::new(RwLock::new(Shared { metadata: None }));

        let transform = Ec2MetadataTransform {
            state: state.clone(),
            ami_id_key: AMI_ID_KEY.clone(),
            availability_zone_key: AVAILABILITY_ZONE_KEY.clone(),
            instance_id_key: INSTANCE_ID_KEY.clone(),
            local_hostname_key: LOCAL_HOSTNAME_KEY.clone(),
            local_ipv4_key: LOCAL_IPV4_KEY.clone(),
            public_hostname_key: PUBLIC_HOSTNAME_KEY.clone(),
            public_ipv4_key: PUBLIC_IPV4_KEY.clone(),
            region_key: REGION_KEY.clone(),
            // role_name_key: ROLE_NAME_KEY.clone(),
            subnet_id_key: SUBNET_ID_KEY.clone(),
            vpc_id_key: VPC_ID_KEY.clone(),
        };

        exec.spawn_std(async move {
            loop {
                if let Err(error) = fetch_metadata(state.clone()).await {
                    error!(message = "Unable to refresh metadata.", %error);
                } else {
                    break;
                }
            }
        });

        Ok(Box::new(transform))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "add_tags"
    }
}

impl Transform for Ec2MetadataTransform {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let metadata = self.state.read().unwrap().clone().metadata;

        if let Some(metadata) = metadata {
            let log = event.as_mut_log();

            log.insert_explicit(self.ami_id_key.clone(), metadata.ami_id.clone().into());
        }

        Some(event)
    }
}

#[derive(Debug, Default, Clone)]
pub struct InstanceMetadata {
    ami_id: Bytes,
    availability_zone: Bytes,
    instance_id: Bytes,
    local_hostname: Bytes,
    local_ipv4: Bytes,
    public_hostname: Bytes,
    public_ipv4: Bytes,
    region: Bytes,
    // role_name: Bytes,
    subnet_id: Bytes,
    vpc_id: Bytes,
}

async fn fetch_metadata(state: Arc<RwLock<Shared>>) -> Result<(), crate::Error> {
    let client = MetadataClient::new(Uri::from_static("http://127.0.0.1:8111"));

    loop {
        // let metadata = InstanceMetadata { ami_id };

        // (*state.write().unwrap()).metadata = Some(metadata);

        let deadline = Instant::now() + Duration::from_secs(2);

        Delay::new(deadline).compat().await?;
    }
}

#[derive(Debug, Clone)]
struct MetadataClient {
    client: Client<HttpConnector, Body>,
    host: Uri,
    token: Option<Bytes>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DynamicIdentityDocument {
    account_id: String,
    architecture: String,
    image_id: String,
    instance_id: String,
    instance_type: String,
    private_ip: String,
    region: String,
    version: String,
}

impl MetadataClient {
    pub fn new(host: Uri) -> Self {
        Self {
            client: Client::new(),
            host,
            token: None,
        }
    }

    pub async fn get_token(&mut self) -> Result<Bytes, crate::Error> {
        if let Some(token) = self.token.clone() {
            Ok(token)
        } else {
            let mut parts = self.host.clone().into_parts();
            parts.path_and_query = Some(API_TOKEN.clone());
            let uri = Uri::from_parts(parts)?;

            let req = Request::put(uri)
                .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
                .body(Body::empty())?;

            let res = self.client.request(req).compat().await?;

            if res.status() != StatusCode::OK {
                unimplemented!("return error here")
            }

            let body = res.into_body().concat2().compat().await?;
            let token = body.into_bytes();

            self.token = Some(token.clone());

            Ok(token)
        }
    }

    pub async fn get_document(&mut self) -> Result<DynamicIdentityDocument, crate::Error> {
        let token = self.get_token().await?;

        let mut parts = self.host.clone().into_parts();
        parts.path_and_query = Some(PathAndQuery::from_static(
            "/latest/dynamic/instance-identity/document",
        ));
        let uri = Uri::from_parts(parts)?;

        let req = Request::get(uri)
            .header("X-aws-ec2-metadata-token", token)
            .body(Body::empty())?;

        let res = self.client.request(req).compat().await?;

        let body = res.into_body().concat2().compat().await?;

        serde_json::from_slice(&body[..]).map_err(Into::into)
    }

    pub async fn get_metadata(&self, path: &PathAndQuery) -> Result<Option<Bytes>, crate::Error> {
        let mut parts = self.host.clone().into_parts();

        parts.path_and_query = Some(path.clone());

        let uri = Uri::from_parts(parts)?;

        info!(message = "sending metadata request.", %uri);

        let res = self.client.get(uri).compat().await?;

        info!(message = "metadata response.", status_code = %res.status());

        if StatusCode::OK != res.status() {
            // TODO: log here
            return Ok(None);
        }

        let body = res.into_body().concat2().compat().await?;

        Ok(Some(body.into_bytes()))
    }

    pub async fn fetch_all(&self) -> Result<(), crate::Error> {
        let ami_id = self.get_metadata(&AMI_ID).await?;
        let availability_zone = self.get_metadata(&AVAILABILITY_ZONE).await?;
        let instance_id = self.get_metadata(&INSTANCE_ID).await?;
        let local_hostname = self.get_metadata(&LOCAL_HOSTNAME).await?;
        let local_ipv4 = self.get_metadata(&LOCAL_IPV4).await?;
        let public_hostname = self.get_metadata(&PUBLIC_HOSTNAME).await?;
        let public_ipv4 = self.get_metadata(&PUBLIC_IPV4).await?;
        // let region = self.get_metadata(&REGION).await?;
        // let subnet_id = self.get_metadata(&SUBNET_ID).await?;
        // let vpc_id = self.get_metadata(&VPC_ID).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::runtime;
    use std::sync::{Arc, RwLock};

    lazy_static::lazy_static! {
        static ref HOST: Uri = Uri::from_static("http://169.254.169.254");
        // static ref HOST: Uri = Uri::from_static("http://localhost:8111");
    }

    #[test]
    fn fetch_dynamic_identity_document() {
        let mut rt = runtime();

        let mut client = MetadataClient::new(HOST.clone());

        let res = rt
            .block_on_std(async move { client.get_document().await })
            .unwrap();
        println!("document {:?}", res);
    }

    #[test]
    fn fetch() {
        crate::test_util::trace_init();

        let mut rt = crate::runtime::Runtime::single_threaded().unwrap();

        let mut client = MetadataClient::new(HOST.clone());

        let ami = rt
            .block_on_std(async move { client.fetch_all().await })
            .unwrap();
    }
}
