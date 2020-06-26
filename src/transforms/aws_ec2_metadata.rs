use super::Transform;
use crate::{
    event::Event,
    hyper::body_to_bytes,
    sinks::util::http::HttpClient,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use bytes::Bytes;
use futures::compat::Future01CompatExt;
use http::{uri::PathAndQuery, Request, StatusCode, Uri};
use hyper::Body;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::RandomState, HashSet};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;
use tokio01::timer::Delay;
use tracing_futures::Instrument;

type WriteHandle = evmap::WriteHandle<Atom, Bytes, (), RandomState>;
type ReadHandle = evmap::ReadHandle<Atom, Bytes, (), RandomState>;

lazy_static::lazy_static! {
    static ref AMI_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/ami-id");
    static ref AMI_ID_KEY: Atom = Atom::from("ami-id");

    static ref AVAILABILITY_ZONE: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/placement/availability-zone");
    static ref AVAILABILITY_ZONE_KEY: Atom = Atom::from("availability-zone");

    static ref INSTANCE_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/instance-id");
    static ref INSTANCE_ID_KEY: Atom = Atom::from("instance-id");

    static ref INSTANCE_TYPE: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/instance-type");
    static ref INSTANCE_TYPE_KEY: Atom = Atom::from("instance-type");

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

    static ref SUBNET_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/network/interfaces/macs/mac/subnet-id");
    static ref SUBNET_ID_KEY: Atom = Atom::from("subnet-id");

    static ref VPC_ID: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/network/interfaces/macs/mac/vpc-id");
    static ref VPC_ID_KEY: Atom = Atom::from("vpc-id");

    static ref ROLE_NAME: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/iam/security-credentials/");
    static ref ROLE_NAME_KEY: Atom = Atom::from("role-name");

    static ref MAC: PathAndQuery = PathAndQuery::from_static("/latest/meta-data/mac");

    static ref DYNAMIC_DOCUMENT: PathAndQuery = PathAndQuery::from_static("/latest/dynamic/instance-identity/document");

    static ref DEFAULT_FIELD_WHITELIST: Vec<Atom> = vec![
        AMI_ID_KEY.clone(),
        AVAILABILITY_ZONE_KEY.clone(),
        INSTANCE_ID_KEY.clone(),
        INSTANCE_TYPE_KEY.clone(),
        LOCAL_HOSTNAME_KEY.clone(),
        LOCAL_IPV4_KEY.clone(),
        PUBLIC_HOSTNAME_KEY.clone(),
        PUBLIC_IPV4_KEY.clone(),
        REGION_KEY.clone(),
        SUBNET_ID_KEY.clone(),
        VPC_ID_KEY.clone(),
        ROLE_NAME_KEY.clone(),
    ];

    static ref API_TOKEN: PathAndQuery = PathAndQuery::from_static("/latest/api/token");
    static ref TOKEN_HEADER: Bytes = Bytes::from("X-aws-ec2-metadata-token");
    static ref HOST: Uri = Uri::from_static("http://169.254.169.254");
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Ec2Metadata {
    host: Option<String>,
    namespace: Option<String>,
    refresh_interval_secs: Option<u64>,
    fields: Option<Vec<String>>,
}

pub struct Ec2MetadataTransform {
    state: ReadHandle,
}

#[derive(Debug, Clone)]
struct Keys {
    ami_id_key: Atom,
    availability_zone_key: Atom,
    instance_id_key: Atom,
    instance_type_key: Atom,
    local_hostname_key: Atom,
    local_ipv4_key: Atom,
    public_hostname_key: Atom,
    public_ipv4_key: Atom,
    region_key: Atom,
    subnet_id_key: Atom,
    vpc_id_key: Atom,
    role_name_key: Atom,
}

inventory::submit! {
    TransformDescription::new_without_default::<Ec2Metadata>("aws_ec2_metadata")
}

#[typetag::serde(name = "aws_ec2_metadata")]
impl TransformConfig for Ec2Metadata {
    fn build(&self, cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let (read, write) = evmap::new();

        // Check if the namespace is set to `""` which should mean that we do
        // not want a prefixed namespace.
        let namespace = self.namespace.clone().and_then(|namespace| {
            if namespace == "" {
                None
            } else {
                Some(namespace)
            }
        });

        let keys = Keys::new(&namespace);

        let host = self
            .host
            .clone()
            .map(|s| Uri::from_maybe_shared(s).unwrap())
            .unwrap_or_else(|| HOST.clone());

        let refresh_interval = self
            .refresh_interval_secs
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(10));
        let fields = self
            .fields
            .clone()
            .map(|v| v.into_iter().map(Atom::from).collect())
            .unwrap_or_else(|| DEFAULT_FIELD_WHITELIST.clone());

        let http_client = HttpClient::new(cx.resolver(), None)?;

        cx.executor().spawn_std(
            async move {
                let mut client =
                    MetadataClient::new(http_client, host, keys, write, refresh_interval, fields);

                client.run().await;
            }
            // TODO: Once #1338 is done we can fetch the current span
            .instrument(info_span!("aws_ec2_metadata: worker")),
        );

        Ok(Box::new(Ec2MetadataTransform { state: read }))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "aws_ec2_metadata"
    }
}

impl Transform for Ec2MetadataTransform {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();

        self.state.for_each(|k, v| {
            if let Some(value) = v.get(0) {
                log.insert(k.clone(), value.clone());
            }
        });

        Some(event)
    }
}

struct MetadataClient {
    client: HttpClient<Body>,
    host: Uri,
    token: Option<(Bytes, Instant)>,
    keys: Keys,
    state: WriteHandle,
    refresh_interval: Duration,
    fields: HashSet<Atom>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityDocument {
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
    pub fn new(
        client: HttpClient<Body>,
        host: Uri,
        keys: Keys,
        state: WriteHandle,
        refresh_interval: Duration,
        fields: Vec<Atom>,
    ) -> Self {
        Self {
            client,
            host,
            token: None,
            keys,
            state,
            refresh_interval,
            fields: fields.into_iter().collect(),
        }
    }

    async fn run(&mut self) {
        loop {
            if let Err(error) = self.refresh_metadata().await {
                error!(message="Unable to fetch EC2 metadata; Retrying.", %error);

                Delay::new(Instant::now() + Duration::from_secs(1))
                    .compat()
                    .await
                    .expect("Timer not set.");

                continue;
            }

            let deadline = Instant::now() + self.refresh_interval;

            Delay::new(deadline).compat().await.expect("Timer not set.");
        }
    }

    pub async fn get_token(&mut self) -> Result<Bytes, crate::Error> {
        if let Some((token, next_refresh)) = self.token.clone() {
            // If the next refresh is greater (in the future) than
            // the current time we can return the token since its still valid
            // otherwise lets refresh it.
            if next_refresh > Instant::now() {
                return Ok(token);
            }
        }

        let mut parts = self.host.clone().into_parts();
        parts.path_and_query = Some(API_TOKEN.clone());
        let uri = Uri::from_parts(parts)?;

        let req = Request::put(uri)
            .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
            .body(Body::empty())?;

        let res = self.client.send(req).await?;

        if res.status() != StatusCode::OK {
            return Err(Ec2MetadataError::UnableToFetchToken.into());
        }

        let token = body_to_bytes(res.into_body()).await?;

        let next_refresh = Instant::now() + Duration::from_secs(21600);
        self.token = Some((token.clone(), next_refresh));

        Ok(token)
    }

    pub async fn get_document(&mut self) -> Result<Option<IdentityDocument>, crate::Error> {
        let token = self.get_token().await?;

        let mut parts = self.host.clone().into_parts();
        parts.path_and_query = Some(DYNAMIC_DOCUMENT.clone());
        let uri = Uri::from_parts(parts)?;

        let req = Request::get(uri)
            .header(TOKEN_HEADER.as_ref(), token.as_ref())
            .body(Body::empty())?;

        let res = self.client.send(req).await?;

        if res.status() != StatusCode::OK {
            warn!(message="Identity document request failed.", status = %res.status());
            return Ok(None);
        }

        let body = body_to_bytes(res.into_body()).await?;

        serde_json::from_slice(&body[..])
            .map_err(Into::into)
            .map(Some)
    }

    pub async fn get_metadata(
        &mut self,
        path: &PathAndQuery,
    ) -> Result<Option<Bytes>, crate::Error> {
        let token = self.get_token().await?;

        let mut parts = self.host.clone().into_parts();

        parts.path_and_query = Some(path.clone());

        let uri = Uri::from_parts(parts)?;

        debug!(message = "Sending metadata request.", %uri);

        let req = Request::get(uri)
            .header(TOKEN_HEADER.as_ref(), token.as_ref())
            .body(Body::empty())?;

        let res = self.client.send(req).await?;

        if StatusCode::OK != res.status() {
            warn!(message="Metadata request failed.", status = %res.status());
            return Ok(None);
        }

        let body = body_to_bytes(res.into_body()).await?;

        Ok(Some(body))
    }

    pub async fn refresh_metadata(&mut self) -> Result<(), crate::Error> {
        // Fetch all resources, _then_ add them to the state map.
        let identity_document = self.get_document().await?;

        if let Some(document) = identity_document {
            if self.fields.contains(&AMI_ID_KEY) {
                self.state
                    .update(self.keys.ami_id_key.clone(), document.image_id.into());
            }

            if self.fields.contains(&INSTANCE_ID_KEY) {
                self.state.update(
                    self.keys.instance_id_key.clone(),
                    document.instance_id.into(),
                );
            }

            if self.fields.contains(&INSTANCE_TYPE_KEY) {
                self.state.update(
                    self.keys.instance_type_key.clone(),
                    document.instance_type.into(),
                );
            }

            if self.fields.contains(&REGION_KEY) {
                self.state
                    .update(self.keys.region_key.clone(), document.region.into());
            }
        }

        if self.fields.contains(&AVAILABILITY_ZONE_KEY) {
            if let Some(availability_zone) = self.get_metadata(&AVAILABILITY_ZONE).await? {
                self.state
                    .update(self.keys.availability_zone_key.clone(), availability_zone);
            }
        }

        if self.fields.contains(&LOCAL_HOSTNAME_KEY) {
            if let Some(local_hostname) = self.get_metadata(&LOCAL_HOSTNAME).await? {
                self.state
                    .update(self.keys.local_hostname_key.clone(), local_hostname);
            }
        }

        if self.fields.contains(&LOCAL_IPV4_KEY) {
            if let Some(local_ipv4) = self.get_metadata(&LOCAL_IPV4).await? {
                self.state
                    .update(self.keys.local_ipv4_key.clone(), local_ipv4);
            }
        }

        if self.fields.contains(&PUBLIC_HOSTNAME_KEY) {
            if let Some(public_hostname) = self.get_metadata(&PUBLIC_HOSTNAME).await? {
                self.state
                    .update(self.keys.public_hostname_key.clone(), public_hostname);
            }
        }

        if self.fields.contains(&PUBLIC_IPV4_KEY) {
            if let Some(public_ipv4) = self.get_metadata(&PUBLIC_IPV4).await? {
                self.state
                    .update(self.keys.public_ipv4_key.clone(), public_ipv4);
            }
        }

        if self.fields.contains(&SUBNET_ID_KEY) || self.fields.contains(&VPC_ID_KEY) {
            if let Some(mac) = self.get_metadata(&MAC).await? {
                let mac = String::from_utf8_lossy(&mac[..]);

                let subnet_path = format!(
                    "/latest/meta-data/network/interfaces/macs/{}/subnet-id",
                    mac
                )
                .parse()?;
                let vpc_path =
                    format!("/latest/meta-data/network/interfaces/macs/{}/vpc-id", mac).parse()?;

                if self.fields.contains(&SUBNET_ID_KEY) {
                    if let Some(subnet_id) = self.get_metadata(&subnet_path).await? {
                        self.state
                            .update(self.keys.subnet_id_key.clone(), subnet_id);
                    }
                }

                if self.fields.contains(&VPC_ID_KEY) {
                    if let Some(vpc_id) = self.get_metadata(&vpc_path).await? {
                        self.state.update(self.keys.vpc_id_key.clone(), vpc_id);
                    }
                }
            }
        }

        if self.fields.contains(&ROLE_NAME_KEY) {
            if let Some(role_names) = self.get_metadata(&ROLE_NAME).await? {
                let role_names = String::from_utf8_lossy(&role_names[..]);

                for (i, role_name) in role_names.lines().enumerate() {
                    self.state.update(
                        format!("{}[{}]", self.keys.role_name_key, i).into(),
                        role_name.into(),
                    );
                }
            }
        }

        // Make changes viewable to the transform. This may block if
        // readers are still reading.
        self.state.refresh();

        Ok(())
    }
}

impl Keys {
    pub fn new(namespace: &Option<String>) -> Self {
        if let Some(namespace) = &namespace {
            Keys {
                ami_id_key: format!("{}.{}", namespace, AMI_ID_KEY.clone()).into(),
                availability_zone_key: format!("{}.{}", namespace, AVAILABILITY_ZONE_KEY.clone())
                    .into(),
                instance_id_key: format!("{}.{}", namespace, INSTANCE_ID_KEY.clone()).into(),
                instance_type_key: format!("{}.{}", namespace, INSTANCE_TYPE_KEY.clone()).into(),
                local_hostname_key: format!("{}.{}", namespace, LOCAL_HOSTNAME_KEY.clone()).into(),
                local_ipv4_key: format!("{}.{}", namespace, LOCAL_IPV4_KEY.clone()).into(),
                public_hostname_key: format!("{}.{}", namespace, PUBLIC_HOSTNAME_KEY.clone())
                    .into(),
                public_ipv4_key: format!("{}.{}", namespace, PUBLIC_IPV4_KEY.clone()).into(),
                region_key: format!("{}.{}", namespace, REGION_KEY.clone()).into(),
                subnet_id_key: format!("{}.{}", namespace, SUBNET_ID_KEY.clone()).into(),
                vpc_id_key: format!("{}.{}", namespace, VPC_ID_KEY.clone()).into(),
                role_name_key: format!("{}.{}", namespace, VPC_ID_KEY.clone()).into(),
            }
        } else {
            Keys {
                ami_id_key: AMI_ID_KEY.clone(),
                availability_zone_key: AVAILABILITY_ZONE_KEY.clone(),
                instance_id_key: INSTANCE_ID_KEY.clone(),
                instance_type_key: INSTANCE_TYPE_KEY.clone(),
                local_hostname_key: LOCAL_HOSTNAME_KEY.clone(),
                local_ipv4_key: LOCAL_IPV4_KEY.clone(),
                public_hostname_key: PUBLIC_HOSTNAME_KEY.clone(),
                public_ipv4_key: PUBLIC_IPV4_KEY.clone(),
                region_key: REGION_KEY.clone(),
                subnet_id_key: SUBNET_ID_KEY.clone(),
                vpc_id_key: VPC_ID_KEY.clone(),
                role_name_key: ROLE_NAME_KEY.clone(),
            }
        }
    }
}

#[derive(Debug, snafu::Snafu)]
enum Ec2MetadataError {
    #[snafu(display("Unable to fetch token."))]
    UnableToFetchToken,
}

#[cfg(feature = "aws-ec2-metadata-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{event::Event, test_util::runtime};

    lazy_static::lazy_static! {
        static ref HOST: String = "http://localhost:8111".to_string();
    }

    #[test]
    fn enrich() {
        crate::test_util::trace_init();
        let rt = runtime();

        let config = Ec2Metadata {
            host: Some(HOST.clone()),
            ..Default::default()
        };
        let mut transform = config
            .build(TransformContext::new_test(rt.executor()))
            .unwrap();

        // We need to sleep to let the background task fetch the data.
        std::thread::sleep(std::time::Duration::from_secs(1));

        let event = Event::new_empty_log();

        let event = transform.transform(event).unwrap();
        let log = event.as_log();

        assert_eq!(
            log.get(&"availability-zone".into()),
            Some(&"ww-region-1a".into())
        );
        assert_eq!(log.get(&"public-ipv4".into()), Some(&"192.1.1.1".into()));
        assert_eq!(
            log.get(&"public-hostname".into()),
            Some(&"mock-public-hostname".into())
        );
        assert_eq!(log.get(&"local-ipv4".into()), Some(&"192.1.1.2".into()));
        assert_eq!(
            log.get(&"local-hostname".into()),
            Some(&"mock-hostname".into())
        );
        assert_eq!(
            log.get(&"instance-id".into()),
            Some(&"i-096fba6d03d36d262".into())
        );
        assert_eq!(
            log.get(&"ami-id".into()),
            Some(&"ami-05f27d4d6770a43d2".into())
        );
        assert_eq!(log.get(&"instance-type".into()), Some(&"t2.micro".into()));
        assert_eq!(log.get(&"region".into()), Some(&"us-east-1".into()));
        assert_eq!(log.get(&"vpc-id".into()), Some(&"mock-vpc-id".into()));
        assert_eq!(log.get(&"subnet-id".into()), Some(&"mock-subnet-id".into()));
        assert_eq!(log.get(&"role-name[0]".into()), Some(&"mock-user".into()));
    }

    #[test]
    fn fields() {
        let rt = runtime();

        let config = Ec2Metadata {
            host: Some(HOST.clone()),
            fields: Some(vec!["public-ipv4".into(), "region".into()]),
            ..Default::default()
        };
        let mut transform = config
            .build(TransformContext::new_test(rt.executor()))
            .unwrap();

        // We need to sleep to let the background task fetch the data.
        std::thread::sleep(std::time::Duration::from_secs(1));

        let event = Event::new_empty_log();

        let event = transform.transform(event).unwrap();
        let log = event.as_log();

        assert_eq!(log.get(&"availability-zone".into()), None);
        assert_eq!(log.get(&"public-ipv4".into()), Some(&"192.1.1.1".into()));
        assert_eq!(log.get(&"public-hostname".into()), None);
        assert_eq!(log.get(&"local-ipv4".into()), None);
        assert_eq!(log.get(&"local-hostname".into()), None);
        assert_eq!(log.get(&"instance-id".into()), None,);
        assert_eq!(log.get(&"instance-type".into()), None,);
        assert_eq!(log.get(&"ami-id".into()), None);
        assert_eq!(log.get(&"region".into()), Some(&"us-east-1".into()));
    }

    #[test]
    fn namespace() {
        let rt = runtime();

        let config = Ec2Metadata {
            host: Some(HOST.clone()),
            namespace: Some("ec2.metadata".into()),
            ..Default::default()
        };
        let mut transform = config
            .build(TransformContext::new_test(rt.executor()))
            .unwrap();

        // We need to sleep to let the background task fetch the data.
        std::thread::sleep(std::time::Duration::from_secs(1));

        let event = Event::new_empty_log();

        let event = transform.transform(event).unwrap();
        let log = event.as_log();

        assert_eq!(
            log.get(&"ec2.metadata.availability-zone".into()),
            Some(&"ww-region-1a".into())
        );
        assert_eq!(
            log.get(&"ec2.metadata.public-ipv4".into()),
            Some(&"192.1.1.1".into())
        );

        // Set an empty namespace to ensure we don't prepend one.
        let config = Ec2Metadata {
            host: Some(HOST.clone()),
            namespace: Some("".into()),
            ..Default::default()
        };
        let mut transform = config
            .build(TransformContext::new_test(rt.executor()))
            .unwrap();

        // We need to sleep to let the background task fetch the data.
        std::thread::sleep(std::time::Duration::from_secs(1));

        let event = Event::new_empty_log();

        let event = transform.transform(event).unwrap();
        let log = event.as_log();

        assert_eq!(
            log.get(&"availability-zone".into()),
            Some(&"ww-region-1a".into())
        );
        assert_eq!(log.get(&"public-ipv4".into()), Some(&"192.1.1.1".into()));
    }
}
