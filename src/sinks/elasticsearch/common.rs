use crate::transforms::metric_to_log::MetricToLog;
use crate::http::{HttpClient, Auth, MaybeAuth};
use crate::sinks::elasticsearch::{finish_signer, ElasticSearchAuth, ElasticSearchConfig, ParseError, maybe_set_id, ElasticSearchCommonMode};
use rusoto_core::signature::SignedRequestPayload;
use crate::sinks::util::http::{HttpSink, RequestConfig};
use crate::event::Event;
use http::{StatusCode, Uri};
use hyper::Body;
use rusoto_signature::SignedRequest;
use crate::tls::TlsSettings;
use crate::sinks::util::{TowerRequestConfig, UriSerde, Compression};
use crate::rusoto::region_from_endpoint;
use std::convert::TryFrom;
use snafu::ResultExt;
use serde_json::json;
use crate::rusoto;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, EncodingConfiguration};
use crate::sinks::elasticsearch::encoder::Encoding;
use std::collections::HashMap;
use rusoto_core::Region;
use super::{Request, InvalidHost};
use crate::sinks::HealthcheckError;

//TODO: Why is this not clone?
#[derive(Debug)]
pub struct ElasticSearchCommon {
    pub base_url: String,
    id_key: Option<String>,
    bulk_uri: Uri,
    authorization: Option<Auth>,
    credentials: Option<rusoto::AwsCredentialsProvider>,
    encoding: EncodingConfigWithDefault<Encoding>,
    mode: ElasticSearchCommonMode,
    doc_type: String,
    tls_settings: TlsSettings,
    compression: Compression,
    region: Region,
    request: RequestConfig,
    query_params: HashMap<String, String>,
    metric_to_log: MetricToLog,
}

impl ElasticSearchCommon {
    fn encode_log(&self, event: Event) -> Option<Vec<u8>> {
        // let index = self.mode.index(&event)?;
        //
        // let mut log = if let Some(cfg) = self.mode.as_data_stream_config() {
        //     cfg.remap_timestamp(cfg.sync_fields(log))
        // } else {
        //     event
        // };
        //
        // let bulk_action = self.mode.bulk_action(&event)?;
        //
        // let mut action = json!({
        //     bulk_action.as_str(): {
        //         "_index": index,
        //         "_type": self.doc_type,
        //     }
        // });
        //
        // maybe_set_id(
        //     self.id_key.as_ref(),
        //     action.pointer_mut(bulk_action.as_json_pointer()).unwrap(),
        //     &mut event,
        // );
        //
        // let mut body = serde_json::to_vec(&action).unwrap();
        // body.push(b'\n');
        //
        // self.encoding.apply_rules(&mut event);
        //
        // serde_json::to_writer(&mut body, &event.into_log()).unwrap();
        // body.push(b'\n');
        //
        // emit!(&ElasticSearchEventEncoded {
        //     byte_size: body.len(),
        //     index,
        // });
        //
        // Some(body)

        // this logic was moved to Encoder
        todo!()
    }
}

impl ElasticSearchCommon {
    pub fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", &config.endpoint);
        let uri = uri.parse::<Uri>().with_context(|| InvalidHost {
            host: &config.endpoint,
        })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: config.endpoint.clone(),
            }
                .into());
        }

        let authorization = match &config.auth {
            Some(ElasticSearchAuth::Basic { user, password }) => Some(Auth::Basic {
                user: user.clone(),
                password: password.clone(),
            }),
            _ => None,
        };
        let uri = config.endpoint.parse::<UriSerde>()?;
        let authorization = authorization.choose_one(&uri.auth)?;
        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();

        let region = match &config.aws {
            Some(region) => Region::try_from(region)?,
            None => region_from_endpoint(&base_url)?,
        };

        let credentials = match &config.auth {
            Some(ElasticSearchAuth::Basic { .. }) | None => None,
            Some(ElasticSearchAuth::Aws(aws)) => Some(aws.build(&region, None)?),
        };

        let compression = config.compression;
        let mode = config.common_mode()?;

        let doc_type = config.doc_type.clone().unwrap_or_else(|| "_doc".into());

        let tower_request = config
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());

        let mut query_params = config.query.clone().unwrap_or_default();
        query_params.insert(
            "timeout".into(),
            format!("{}s", tower_request.timeout.as_secs()),
        );

        if let Some(pipeline) = &config.pipeline {
            query_params.insert("pipeline".into(), pipeline.into());
        }

        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (p, v) in &query_params {
            query.append_pair(&p[..], &v[..]);
        }
        let bulk_url = format!("{}/_bulk?{}", base_url, query.finish());
        let bulk_uri = bulk_url.parse::<Uri>().unwrap();

        let tls_settings = TlsSettings::from_options(&config.tls)?;
        let mut config = config.clone();
        let mut request = config.request;
        request.add_old_option(config.headers.take());

        let metric_config = config.metrics.clone().unwrap_or_default();
        let metric_to_log = MetricToLog::new(
            metric_config.host_tag,
            metric_config.timezone.unwrap_or_default(),
        );

        Ok(Self {
            authorization,
            base_url,
            bulk_uri,
            compression,
            credentials,
            doc_type,
            encoding: config.encoding,
            id_key: config.id_key,
            mode,
            query_params,
            request,
            region,
            tls_settings,
            metric_to_log,
        })
    }

    fn signed_request(&self, method: &str, uri: &Uri, use_params: bool) -> SignedRequest {
        let mut request = SignedRequest::new(method, "es", &self.region, uri.path());
        request.set_hostname(uri.host().map(|host| host.into()));
        if use_params {
            for (key, value) in &self.query_params {
                request.add_param(key, value);
            }
        }
        request
    }

    //TODO: migrate away from HttpClient?
    async fn healthcheck(self, client: HttpClient) -> crate::Result<()> {
        let mut builder = Request::get(format!("{}/_cluster/health", self.base_url));

        match &self.credentials {
            None => {
                if let Some(authorization) = &self.authorization {
                    builder = authorization.apply_builder(builder);
                }
            }
            Some(credentials_provider) => {
                let mut signer = self.signed_request("GET", builder.uri_ref().unwrap(), false);
                builder = finish_signer(&mut signer, credentials_provider, builder).await?;
            }
        }
        let request = builder.body(Body::empty())?;
        let response = client.send(request).await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
        }
    }
}

//TODO: This might be used by other sinks?
#[async_trait::async_trait]
impl HttpSink for ElasticSearchCommon {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let log = match event {
            Event::Log(log) => Some(log),
            Event::Metric(metric) => self.metric_to_log.transform_one(metric),
        };
        log.and_then(|log| self.encode_log(log.into()))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let mut builder = Request::post(&self.bulk_uri);

        if let Some(credentials_provider) = &self.credentials {
            let mut request = self.signed_request("POST", &self.bulk_uri, true);

            request.add_header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                request.add_header("Content-Encoding", ce);
            }

            for (header, value) in &self.request.headers {
                request.add_header(header, value);
            }

            request.set_payload(Some(events));

            // mut builder?
            builder = finish_signer(&mut request, credentials_provider, builder).await?;

            // The SignedRequest ends up owning the body, so we have
            // to play games here
            let body = request.payload.take().unwrap();
            match body {
                SignedRequestPayload::Buffer(body) => {
                    builder.body(body.to_vec()).map_err(Into::into)
                }
                _ => unreachable!(),
            }
        } else {
            builder = builder.header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            for (header, value) in &self.request.headers {
                builder = builder.header(&header[..], &value[..]);
            }

            if let Some(auth) = &self.authorization {
                builder = auth.apply_builder(builder);
            }

            builder.body(events).map_err(Into::into)
        }
    }
}
