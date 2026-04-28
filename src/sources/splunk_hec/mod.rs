use std::{
    collections::HashMap,
    convert::Infallible,
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use chrono::{DateTime, TimeZone, Utc};
use flate2::read::MultiGzDecoder;
use futures::FutureExt;
use http::StatusCode;
use hyper::{Server, service::make_service_fn};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{
    Deserializer, Value as JsonValue,
    de::{Read as JsonRead, StrRead},
};
use snafu::Snafu;
use tokio::net::TcpStream;
use tokio_util::codec::Decoder as _;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        Decoder, StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    event::BatchNotifier,
    internal_event::{CountByteSize, InternalEventHandle as _, Registered},
    lookup::{self, event_path, lookup_v2::OptionalValuePath, metadata_path, owned_value_path},
    schema::meaning,
    sensitive_string::SensitiveString,
    source_sender::SendError,
    tls::MaybeTlsIncomingStream,
};
use vrl::{
    path::{OwnedTargetPath, PathPrefix},
    value::{Kind, kind::Collection},
};
use warp::{
    Filter, Reply,
    filters::BoxedFilter,
    http::header::{CONTENT_TYPE, HeaderValue},
    path,
    reject::Rejection,
    reply::Response,
};

use self::{
    acknowledgements::{
        HecAckStatusRequest, HecAckStatusResponse, HecAcknowledgementsConfig,
        IndexerAcknowledgement,
    },
    splunk_response::{HecResponse, HecResponseMetadata, HecStatusCode},
};
use crate::{
    SourceSender,
    codecs::DecodingConfig,
    config::{DataType, Resource, SourceConfig, SourceContext, SourceOutput, log_schema},
    event::{Event, LogEvent, Value},
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer},
    internal_events::{
        EventsReceived, HttpBytesReceived, SplunkHecRequestBodyInvalidError, SplunkHecRequestError,
    },
    serde::bool_or_struct,
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

mod acknowledgements;

// Event fields unique to splunk_hec source
pub const CHANNEL: &str = "splunk_channel";
pub const INDEX: &str = "splunk_index";
pub const SOURCE: &str = "splunk_source";
pub const SOURCETYPE: &str = "splunk_sourcetype";

const X_SPLUNK_REQUEST_CHANNEL: &str = "x-splunk-request-channel";

/// Configuration for the `splunk_hec` source.
#[configurable_component(source("splunk_hec", "Receive logs from Splunk."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct SplunkConfig {
    /// The socket address to listen for connections on.
    ///
    /// The address _must_ include a port.
    #[serde(default = "default_socket_address")]
    pub address: SocketAddr,

    /// Optional authorization token.
    ///
    /// If supplied, incoming requests must supply this token in the `Authorization` header, just as a client would if
    /// it was communicating with the Splunk HEC endpoint directly.
    ///
    /// If _not_ supplied, the `Authorization` header is ignored and requests are not authenticated.
    #[configurable(deprecated = "This option has been deprecated, use `valid_tokens` instead.")]
    token: Option<SensitiveString>,

    /// A list of valid authorization tokens.
    ///
    /// If supplied, incoming requests must supply one of these tokens in the `Authorization` header, just as a client
    /// would if it was communicating with the Splunk HEC endpoint directly.
    ///
    /// If _not_ supplied, the `Authorization` header is ignored and requests are not authenticated.
    #[configurable(metadata(docs::examples = "A94A8FE5CCB19BA61C4C08"))]
    valid_tokens: Option<Vec<SensitiveString>>,

    /// Whether or not to forward the Splunk HEC authentication token with events.
    ///
    /// If set to `true`, when incoming requests contain a Splunk HEC token, the token used is kept in the
    /// event metadata and preferentially used if the event is sent to a Splunk HEC sink.
    store_hec_token: bool,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(deserialize_with = "bool_or_struct")]
    acknowledgements: HecAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global settings.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,

    /// Codec configuration applied to events received on `/services/collector/event`.
    ///
    /// When `decoding` is set, Vector applies a second decoding pass after the HEC
    /// envelope is parsed: the envelope's `event` field is fed through the codec
    /// (string contents are passed as raw bytes; objects, arrays, and other JSON
    /// values are JSON-serialized first to preserve shape), and a single envelope
    /// can fan out to multiple events. When unset, the endpoint preserves its
    /// existing behavior.
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub event: EndpointCodecConfig,

    /// Codec configuration applied to events received on `/services/collector/raw`.
    ///
    /// When `decoding` is set, the (decompressed) request body is fed through the
    /// codec instead of being emitted as a single event. When unset, the endpoint
    /// preserves its existing behavior of one event per request body.
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub raw: EndpointCodecConfig,
}

/// Codec configuration applied to one of the `splunk_hec` endpoints.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct EndpointCodecConfig {
    /// Framing configuration applied to the payload.
    ///
    /// Only used when `decoding` is also set. Defaults to a per-codec choice
    /// (typically `bytes`) that produces one event per payload.
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub framing: Option<FramingConfig>,

    /// Decoding configuration applied to the payload.
    ///
    /// When unset, the endpoint preserves its existing per-endpoint default
    /// behavior. When set, the inner payload is run through `framing` + `decoding`
    /// and a single payload can fan out to multiple events.
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub decoding: Option<DeserializerConfig>,
}

impl EndpointCodecConfig {
    fn build_decoder(&self, log_namespace: LogNamespace) -> crate::Result<Option<Decoder>> {
        match &self.decoding {
            Some(decoding) => {
                let framing = self
                    .framing
                    .clone()
                    .unwrap_or_else(|| decoding.default_message_based_framing());
                Ok(Some(
                    DecodingConfig::new(framing, decoding.clone(), log_namespace).build()?,
                ))
            }
            None => Ok(None),
        }
    }
}

impl_generate_config_from_default!(SplunkConfig);

impl Default for SplunkConfig {
    fn default() -> Self {
        SplunkConfig {
            address: default_socket_address(),
            token: None,
            valid_tokens: None,
            tls: None,
            acknowledgements: Default::default(),
            store_hec_token: false,
            log_namespace: None,
            keepalive: Default::default(),
            event: EndpointCodecConfig::default(),
            raw: EndpointCodecConfig::default(),
        }
    }
}

fn default_socket_address() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 8088)
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec")]
impl SourceConfig for SplunkConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), true)?;
        let shutdown = cx.shutdown.clone();
        let out = cx.out.clone();
        let log_namespace = cx.log_namespace(self.log_namespace);
        let event_decoder = self.event.build_decoder(log_namespace)?;
        let raw_decoder = self.raw.build_decoder(log_namespace)?;
        let source = SplunkSource::new(
            self,
            tls.http_protocol_name(),
            event_decoder,
            raw_decoder,
            cx,
        );

        let event_service = source.event_service(out.clone());
        let raw_service = source.raw_service(out);
        let health_service = source.health_service();
        let ack_service = source.ack_service();
        let options = SplunkSource::options();

        let services = path!("services" / "collector" / ..)
            .and(
                event_service
                    .or(raw_service)
                    .unify()
                    .or(health_service)
                    .unify()
                    .or(ack_service)
                    .unify()
                    .or(options)
                    .unify(),
            )
            .or_else(finish_err);

        let listener = tls.bind(&self.address).await?;

        let keepalive_settings = self.keepalive.clone();
        Ok(Box::pin(async move {
            let span = Span::current();
            let make_svc = make_service_fn(move |conn: &MaybeTlsIncomingStream<TcpStream>| {
                let svc = ServiceBuilder::new()
                    .layer(build_http_trace_layer(span.clone()))
                    .option_layer(keepalive_settings.max_connection_age_secs.map(|secs| {
                        MaxConnectionAgeLayer::new(
                            Duration::from_secs(secs),
                            keepalive_settings.max_connection_age_jitter_factor,
                            conn.peer_addr(),
                        )
                    }))
                    .service(warp::service(services.clone()));
                futures_util::future::ok::<_, Infallible>(svc)
            });

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(make_svc)
                .with_graceful_shutdown(shutdown.map(|_| ()))
                .await
                .map_err(|err| {
                    error!("An error occurred: {:?}.", err);
                })?;

            Ok(())
        }))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        // Build the base schema as the union of the per-endpoint decoder schemas.
        // When an endpoint has no decoder, fall back to the legacy hardcoded shape -
        // since events from BOTH endpoints flow through the same SourceOutput, the
        // schema needs to express the union of every shape Vector might emit.
        let legacy_base = || match log_namespace {
            LogNamespace::Legacy => {
                let definition = vector_lib::schema::Definition::empty_legacy_namespace()
                    .with_event_field(
                        &owned_value_path!("line"),
                        Kind::object(Collection::empty())
                            .or_array(Collection::empty())
                            .or_undefined(),
                        None,
                    );

                if let Some(message_key) = log_schema().message_key() {
                    definition.with_event_field(
                        message_key,
                        Kind::bytes().or_undefined(),
                        Some(meaning::MESSAGE),
                    )
                } else {
                    definition
                }
            }
            LogNamespace::Vector => vector_lib::schema::Definition::new_with_default_metadata(
                Kind::bytes().or_object(Collection::empty()),
                [log_namespace],
            )
            .with_meaning(OwnedTargetPath::event_root(), meaning::MESSAGE),
        };

        // When only one endpoint has a decoder, the other endpoint still emits the
        // legacy log shape - so the schema needs to express both possibilities.
        let base = match (&self.event.decoding, &self.raw.decoding) {
            (None, None) => legacy_base(),
            (Some(d), None) | (None, Some(d)) => {
                d.schema_definition(log_namespace).merge(legacy_base())
            }
            (Some(de), Some(dr)) => de
                .schema_definition(log_namespace)
                .merge(dr.schema_definition(log_namespace)),
        };

        let schema_definition = base
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                SplunkConfig::NAME,
                log_schema()
                    .host_key()
                    .cloned()
                    .map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("host"),
                Kind::bytes(),
                Some(meaning::HOST),
            )
            .with_source_metadata(
                SplunkConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(CHANNEL))),
                &owned_value_path!("channel"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                SplunkConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(INDEX))),
                &owned_value_path!("index"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                SplunkConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(SOURCE))),
                &owned_value_path!("source"),
                Kind::bytes(),
                Some(meaning::SERVICE),
            )
            // Not to be confused with `source_type`.
            .with_source_metadata(
                SplunkConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(SOURCETYPE))),
                &owned_value_path!("sourcetype"),
                Kind::bytes(),
                None,
            );

        // Output type is the union of both endpoints' decoder output types
        // (logs from a JSON codec, metrics from native, etc.). The legacy path
        // always emits logs, so when an endpoint has no decoder we OR `Log` in.
        let output_type = match (&self.event.decoding, &self.raw.decoding) {
            (None, None) => DataType::Log,
            (Some(d), None) | (None, Some(d)) => d.output_type() | DataType::Log,
            (Some(de), Some(dr)) => de.output_type() | dr.output_type(),
        };
        vec![SourceOutput::new_maybe_logs(output_type, schema_definition)]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

/// Shared data for responding to requests.
struct SplunkSource {
    valid_credentials: Vec<String>,
    protocol: &'static str,
    idx_ack: Option<Arc<IndexerAcknowledgement>>,
    store_hec_token: bool,
    log_namespace: LogNamespace,
    events_received: Registered<EventsReceived>,
    event_decoder: Option<Decoder>,
    raw_decoder: Option<Decoder>,
}

impl SplunkSource {
    fn new(
        config: &SplunkConfig,
        protocol: &'static str,
        event_decoder: Option<Decoder>,
        raw_decoder: Option<Decoder>,
        cx: SourceContext,
    ) -> Self {
        let log_namespace = cx.log_namespace(config.log_namespace);
        let acknowledgements = cx.do_acknowledgements(config.acknowledgements.enabled.into());
        let shutdown = cx.shutdown;
        let valid_tokens = config
            .valid_tokens
            .iter()
            .flatten()
            .chain(config.token.iter());

        let idx_ack = acknowledgements.then(|| {
            Arc::new(IndexerAcknowledgement::new(
                config.acknowledgements.clone(),
                shutdown,
            ))
        });

        SplunkSource {
            valid_credentials: valid_tokens
                .map(|token| format!("Splunk {}", token.inner()))
                .collect(),
            protocol,
            idx_ack,
            store_hec_token: config.store_hec_token,
            log_namespace,
            events_received: register!(EventsReceived),
            event_decoder,
            raw_decoder,
        }
    }

    fn event_service(&self, out: SourceSender) -> BoxedFilter<(Response,)> {
        let splunk_channel_query_param = warp::query::<HashMap<String, String>>()
            .map(|qs: HashMap<String, String>| qs.get("channel").map(|v| v.to_owned()));
        let splunk_channel_header = warp::header::optional::<String>(X_SPLUNK_REQUEST_CHANNEL);

        let splunk_channel = splunk_channel_header
            .and(splunk_channel_query_param)
            .map(|header: Option<String>, query_param| header.or(query_param));

        let protocol = self.protocol;
        let idx_ack = self.idx_ack.clone();
        let store_hec_token = self.store_hec_token;
        let log_namespace = self.log_namespace;
        let events_received = self.events_received.clone();
        let decoder = self.event_decoder.clone();

        warp::post()
            .and(
                path!("event")
                    .or(path!("event" / "1.0"))
                    .or(warp::path::end()),
            )
            .and(self.authorization())
            .and(splunk_channel)
            .and(warp::addr::remote())
            .and(warp::header::optional::<String>("X-Forwarded-For"))
            .and(self.gzip())
            .and(warp::body::bytes())
            .and(warp::path::full())
            .and_then(
                move |_,
                      token: Option<String>,
                      channel: Option<String>,
                      remote: Option<SocketAddr>,
                      remote_addr: Option<String>,
                      gzip: bool,
                      body: Bytes,
                      path: warp::path::FullPath| {
                    let mut out = out.clone();
                    let idx_ack = idx_ack.clone();
                    let events_received = events_received.clone();
                    let decoder = decoder.clone();

                    async move {
                        if idx_ack.is_some() && channel.is_none() {
                            return Err(Rejection::from(ApiError::MissingChannel));
                        }

                        let mut data = Vec::new();
                        let (byte_size, body) = if gzip {
                            MultiGzDecoder::new(body.reader())
                                .read_to_end(&mut data)
                                .map_err(|_| Rejection::from(ApiError::BadRequest))?;
                            (data.len(), String::from_utf8_lossy(data.as_slice()))
                        } else {
                            (body.len(), String::from_utf8_lossy(body.as_ref()))
                        };
                        emit!(HttpBytesReceived {
                            byte_size,
                            http_path: path.as_str(),
                            protocol,
                        });

                        let (batch, mut receiver) =
                            BatchNotifier::maybe_new_with_receiver(idx_ack.is_some());
                        let decoder_in_use = decoder.is_some();

                        // Without a decoder, register the ack id BEFORE iteration so
                        // capacity-exhaustion (`ServiceUnavailable`) short-circuits
                        // the request without parsing the body - byte-for-byte parity
                        // with the pre-decoder behavior.
                        let mut maybe_ack_id = None;
                        if !decoder_in_use {
                            maybe_ack_id = match (idx_ack.clone(), receiver.take(), channel.clone())
                            {
                                (Some(idx_ack), Some(rx), Some(channel_id)) => {
                                    match idx_ack.get_ack_id_from_channel(channel_id, rx).await {
                                        Ok(ack_id) => Some(ack_id),
                                        Err(rej) => return Err(rej),
                                    }
                                }
                                _ => None,
                            };
                        }

                        let mut error = None;
                        let mut events = Vec::new();
                        let mut had_decode_errors = false;

                        let iter: EventIterator<'_, StrRead<'_>> = EventIteratorGenerator {
                            deserializer: Deserializer::from_str(&body).into_iter::<JsonValue>(),
                            channel: channel.clone(),
                            remote,
                            remote_addr,
                            batch,
                            token: token.filter(|_| store_hec_token).map(Into::into),
                            log_namespace,
                            events_received,
                            decoder,
                        }
                        .into();

                        for result in iter {
                            match result {
                                Ok((chunk, errored)) => {
                                    events.extend(chunk);
                                    had_decode_errors |= errored;
                                }
                                Err(err) => {
                                    error = Some(err);
                                    break;
                                }
                            }
                        }

                        // With a decoder, defer ack registration until we know whether
                        // the codec emitted anything *and* whether it dropped any
                        // frames. Returning an ack id when the decoder silently lost
                        // data would let `/services/collector/ack` report success for
                        // a request that lost rows mid-stream.
                        if decoder_in_use {
                            maybe_ack_id = if events.is_empty() || had_decode_errors {
                                drop(receiver);
                                None
                            } else {
                                match (idx_ack, receiver, channel) {
                                    (Some(idx_ack), Some(receiver), Some(channel_id)) => {
                                        match idx_ack
                                            .get_ack_id_from_channel(channel_id, receiver)
                                            .await
                                        {
                                            Ok(ack_id) => Some(ack_id),
                                            Err(rej) => return Err(rej),
                                        }
                                    }
                                    _ => None,
                                }
                            };
                        }

                        if !events.is_empty() {
                            match out.send_batch(events).await {
                                Ok(()) => (),
                                Err(SendError::Closed) => {
                                    return Err(Rejection::from(ApiError::ServerShutdown));
                                }
                                Err(SendError::Timeout) => {
                                    unreachable!("No timeout is configured for this source.")
                                }
                            }
                        }

                        if let Some(error) = error {
                            Err(error)
                        } else {
                            Ok(maybe_ack_id)
                        }
                    }
                },
            )
            .map(finish_ok)
            .boxed()
    }

    fn raw_service(&self, out: SourceSender) -> BoxedFilter<(Response,)> {
        let protocol = self.protocol;
        let idx_ack = self.idx_ack.clone();
        let store_hec_token = self.store_hec_token;
        let events_received = self.events_received.clone();
        let log_namespace = self.log_namespace;
        let decoder = self.raw_decoder.clone();

        warp::post()
            .and(path!("raw" / "1.0").or(path!("raw")))
            .and(self.authorization())
            .and(SplunkSource::required_channel())
            .and(warp::addr::remote())
            .and(warp::header::optional::<String>("X-Forwarded-For"))
            .and(self.gzip())
            .and(warp::body::bytes())
            .and(warp::path::full())
            .and_then(
                move |_,
                      token: Option<String>,
                      channel_id: String,
                      remote: Option<SocketAddr>,
                      xff: Option<String>,
                      gzip: bool,
                      body: Bytes,
                      path: warp::path::FullPath| {
                    let mut out = out.clone();
                    let idx_ack = idx_ack.clone();
                    let events_received = events_received.clone();
                    let decoder = decoder.clone();
                    emit!(HttpBytesReceived {
                        byte_size: body.len(),
                        http_path: path.as_str(),
                        protocol,
                    });

                    async move {
                        let (batch, receiver) =
                            BatchNotifier::maybe_new_with_receiver(idx_ack.is_some());

                        // No-decoder path: byte-for-byte identical to the pre-decoder
                        // code - register ack first (fast-fail under capacity
                        // exhaustion), build a single event, send via `send_event`
                        // (avoids `send_batch_latency` emission).
                        let Some(decoder) = decoder else {
                            let maybe_ack_id = match (idx_ack, receiver) {
                                (Some(idx_ack), Some(receiver)) => Some(
                                    idx_ack
                                        .get_ack_id_from_channel(channel_id.clone(), receiver)
                                        .await?,
                                ),
                                _ => None,
                            };
                            let (mut events, _) = raw_event(
                                body,
                                gzip,
                                channel_id,
                                remote,
                                xff,
                                batch,
                                log_namespace,
                                &events_received,
                                None,
                            )?;
                            // raw_event with no decoder always produces exactly one
                            // event.
                            let mut event = events.pop().expect(
                                "raw_event always produces a single event when no decoder is set",
                            );
                            if let Some(token) = token.filter(|_| store_hec_token) {
                                event.metadata_mut().set_splunk_hec_token(token.into());
                            }
                            let res = out.send_event(event).await;
                            return res
                                .map(|_| maybe_ack_id)
                                .map_err(|_| Rejection::from(ApiError::ServerShutdown));
                        };

                        // Decoder path: build events first, then decide whether to
                        // ack based on whether the codec produced anything and
                        // whether it dropped any frames mid-stream.
                        let (mut events, had_decode_errors) = raw_event(
                            body,
                            gzip,
                            channel_id.clone(),
                            remote,
                            xff,
                            batch,
                            log_namespace,
                            &events_received,
                            Some(decoder),
                        )?;
                        if let Some(token) = token.filter(|_| store_hec_token) {
                            let token: Arc<str> = Arc::from(token);
                            for event in &mut events {
                                event
                                    .metadata_mut()
                                    .set_splunk_hec_token(Arc::clone(&token));
                            }
                        }

                        if events.is_empty() || had_decode_errors {
                            // With newline framing, `valid \n invalid \n valid`
                            // decodes to two events plus one dropped frame; returning
                            // an ack id there would let `/services/collector/ack`
                            // report success for data Vector silently lost.
                            drop(receiver);
                            if events.is_empty() {
                                return Ok(None);
                            }
                            // Forward the partial events with no ack so the source's
                            // existing partial-delivery semantics still apply.
                            let res = out.send_batch(events).await;
                            return res
                                .map(|_| None)
                                .map_err(|_| Rejection::from(ApiError::ServerShutdown));
                        }

                        let maybe_ack_id = match (idx_ack, receiver) {
                            (Some(idx_ack), Some(receiver)) => Some(
                                idx_ack
                                    .get_ack_id_from_channel(channel_id, receiver)
                                    .await?,
                            ),
                            _ => None,
                        };

                        let res = out.send_batch(events).await;
                        res.map(|_| maybe_ack_id)
                            .map_err(|_| Rejection::from(ApiError::ServerShutdown))
                    }
                },
            )
            .map(finish_ok)
            .boxed()
    }

    fn health_service(&self) -> BoxedFilter<(Response,)> {
        // The Splunk docs document this endpoint as returning a 400 if given an invalid Splunk
        // token, but, in practice, it seems to ignore the token altogether
        //
        // The response body was taken from Splunk 8.2.4
        //
        // https://docs.splunk.com/Documentation/Splunk/8.2.5/RESTREF/RESTinput#services.2Fcollector.2Fhealth
        warp::get()
            .and(path!("health" / "1.0").or(path!("health")))
            .map(move |_| {
                http::Response::builder()
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(hyper::Body::from(r#"{"text":"HEC is healthy","code":17}"#))
                    .expect("static response")
            })
            .boxed()
    }

    fn lenient_json_content_type_check<T>() -> impl Filter<Extract = (T,), Error = Rejection> + Clone
    where
        T: Send + DeserializeOwned + 'static,
    {
        warp::header::optional::<HeaderValue>(CONTENT_TYPE.as_str())
            .and(warp::body::bytes())
            .and_then(
                |ctype: Option<HeaderValue>, body: bytes::Bytes| async move {
                    let ok = ctype
                        .as_ref()
                        .and_then(|v| v.to_str().ok())
                        .map(|h| h.to_ascii_lowercase().contains("application/json"))
                        .unwrap_or(true);

                    if !ok {
                        return Err(warp::reject::custom(ApiError::UnsupportedContentType));
                    }

                    let value = serde_json::from_slice::<T>(&body)
                        .map_err(|_| warp::reject::custom(ApiError::BadRequest))?;

                    Ok(value)
                },
            )
    }

    fn ack_service(&self) -> BoxedFilter<(Response,)> {
        let idx_ack = self.idx_ack.clone();

        warp::post()
            .and(warp::path!("ack"))
            .and(self.authorization())
            .and(SplunkSource::required_channel())
            .and(Self::lenient_json_content_type_check::<HecAckStatusRequest>())
            .and_then(move |_, channel: String, req: HecAckStatusRequest| {
                let idx_ack = idx_ack.clone();
                async move {
                    if let Some(idx_ack) = idx_ack {
                        let acks = idx_ack
                            .get_acks_status_from_channel(channel, &req.acks)
                            .await?;
                        Ok(warp::reply::json(&HecAckStatusResponse { acks }).into_response())
                    } else {
                        Err(warp::reject::custom(ApiError::AckIsDisabled))
                    }
                }
            })
            .boxed()
    }

    fn options() -> BoxedFilter<(Response,)> {
        let post = warp::options()
            .and(
                path!("event")
                    .or(path!("event" / "1.0"))
                    .or(path!("raw" / "1.0"))
                    .or(path!("raw")),
            )
            .map(|_| warp::reply::with_header(warp::reply(), "Allow", "POST").into_response());

        let get = warp::options()
            .and(path!("health").or(path!("health" / "1.0")))
            .map(|_| warp::reply::with_header(warp::reply(), "Allow", "GET").into_response());

        post.or(get).unify().boxed()
    }

    /// Authorize request
    fn authorization(&self) -> BoxedFilter<(Option<String>,)> {
        let valid_credentials = self.valid_credentials.clone();
        warp::header::optional("Authorization")
            .and_then(move |token: Option<String>| {
                let valid_credentials = valid_credentials.clone();
                async move {
                    match (token, valid_credentials.is_empty()) {
                        // Remove the "Splunk " prefix if present as it is not
                        // part of the token itself
                        (token, true) => {
                            Ok(token
                                .map(|t| t.strip_prefix("Splunk ").map(Into::into).unwrap_or(t)))
                        }
                        (Some(token), false) if valid_credentials.contains(&token) => Ok(Some(
                            token
                                .strip_prefix("Splunk ")
                                .map(Into::into)
                                .unwrap_or(token),
                        )),
                        (Some(_), false) => Err(Rejection::from(ApiError::InvalidAuthorization)),
                        (None, false) => Err(Rejection::from(ApiError::MissingAuthorization)),
                    }
                }
            })
            .boxed()
    }

    /// Is body encoded with gzip
    fn gzip(&self) -> BoxedFilter<(bool,)> {
        warp::header::optional::<String>("Content-Encoding")
            .and_then(|encoding: Option<String>| async move {
                match encoding {
                    Some(s) if s.as_bytes() == b"gzip" => Ok(true),
                    Some(_) => Err(Rejection::from(ApiError::UnsupportedEncoding)),
                    None => Ok(false),
                }
            })
            .boxed()
    }

    fn required_channel() -> BoxedFilter<(String,)> {
        let splunk_channel_query_param = warp::query::<HashMap<String, String>>()
            .map(|qs: HashMap<String, String>| qs.get("channel").map(|v| v.to_owned()));
        let splunk_channel_header = warp::header::optional::<String>(X_SPLUNK_REQUEST_CHANNEL);

        splunk_channel_header
            .and(splunk_channel_query_param)
            .and_then(|header: Option<String>, query_param| async move {
                header
                    .or(query_param)
                    .ok_or_else(|| Rejection::from(ApiError::MissingChannel))
            })
            .boxed()
    }
}
/// Constructs one or more events from json-s coming from reader.
/// If errors, it's done with input.
struct EventIterator<'de, R: JsonRead<'de>> {
    /// Remaining request with JSON events
    deserializer: serde_json::StreamDeserializer<'de, R, JsonValue>,
    /// Count of HEC envelopes (not fan-out events) processed so far. Used both as the
    /// `InvalidEventNumber` index in Splunk error responses (zero-indexed: subtract 1
    /// for build-time errors, use as-is for parse errors that haven't entered build)
    /// and as the "did we see any envelope?" check that gates the `NoData` error.
    envelopes_processed: usize,
    /// Optional channel from headers
    channel: Option<Value>,
    /// Default time
    time: Time,
    /// Remaining extracted default values
    extractors: [DefaultExtractor; 4],
    /// Event finalization
    batch: Option<BatchNotifier>,
    /// Splunk HEC Token for passthrough
    token: Option<Arc<str>>,
    /// Lognamespace to put the events in
    log_namespace: LogNamespace,
    /// handle to EventsReceived registry
    events_received: Registered<EventsReceived>,
    /// Optional second-stage decoder applied to the envelope payload.
    decoder: Option<Decoder>,
}

/// Intermediate struct to generate an `EventIterator`
struct EventIteratorGenerator<'de, R: JsonRead<'de>> {
    deserializer: serde_json::StreamDeserializer<'de, R, JsonValue>,
    channel: Option<String>,
    batch: Option<BatchNotifier>,
    token: Option<Arc<str>>,
    log_namespace: LogNamespace,
    events_received: Registered<EventsReceived>,
    remote: Option<SocketAddr>,
    remote_addr: Option<String>,
    decoder: Option<Decoder>,
}

impl<'de, R: JsonRead<'de>> From<EventIteratorGenerator<'de, R>> for EventIterator<'de, R> {
    fn from(f: EventIteratorGenerator<'de, R>) -> Self {
        // The host field can collide with decoder-produced output in legacy namespace
        // (its legacy key is `log_schema().host_key()`, typically `"host"`). When a
        // decoder is configured, prefer the decoder's value over the envelope's so the
        // user's parsed view wins on conflict. With no decoder configured, behavior is
        // unchanged: every extractor uses `Overwrite`.
        let extractor_strategy = if f.decoder.is_some() {
            LegacyKeyStrategy::InsertIfEmpty
        } else {
            LegacyKeyStrategy::Overwrite
        };
        Self {
            deserializer: f.deserializer,
            envelopes_processed: 0,
            channel: f.channel.map(Value::from),
            time: Time::Now(Utc::now()),
            extractors: [
                // Extract the host field with the given priority:
                // 1. The host field is present in the event payload
                // 2. The x-forwarded-for header is present in the incoming request
                // 3. Use the `remote`: SocketAddr value provided by warp
                DefaultExtractor::new_with(
                    "host",
                    log_schema().host_key().cloned().into(),
                    f.remote_addr
                        .or_else(|| f.remote.map(|addr| addr.to_string()))
                        .map(Value::from),
                    f.log_namespace,
                )
                .with_legacy_key_strategy(extractor_strategy),
                DefaultExtractor::new("index", OptionalValuePath::new(INDEX), f.log_namespace)
                    .with_legacy_key_strategy(extractor_strategy),
                DefaultExtractor::new("source", OptionalValuePath::new(SOURCE), f.log_namespace)
                    .with_legacy_key_strategy(extractor_strategy),
                DefaultExtractor::new(
                    "sourcetype",
                    OptionalValuePath::new(SOURCETYPE),
                    f.log_namespace,
                )
                .with_legacy_key_strategy(extractor_strategy),
            ],
            batch: f.batch,
            token: f.token,
            log_namespace: f.log_namespace,
            events_received: f.events_received,
            decoder: f.decoder,
        }
    }
}

impl<'de, R: JsonRead<'de>> EventIterator<'de, R> {
    /// Process the envelope's `time` field, updating `self.time` (sticky across envelopes
    /// when not explicitly provided).
    fn process_time(&mut self, json: &mut JsonValue) -> Result<(), Rejection> {
        let parsed_time = match json.get_mut("time").map(JsonValue::take) {
            Some(JsonValue::Number(time)) => Some(Some(time)),
            Some(JsonValue::String(time)) => Some(time.parse::<serde_json::Number>().ok()),
            _ => None,
        };

        match parsed_time {
            None => Ok(()),
            Some(Some(t)) => {
                if let Some(t) = t.as_u64() {
                    let time = parse_timestamp(t as i64).ok_or(ApiError::InvalidDataFormat {
                        event: self.envelopes_processed.saturating_sub(1),
                    })?;
                    self.time = Time::Provided(time);
                    Ok(())
                } else if let Some(t) = t.as_f64() {
                    self.time = Time::Provided(
                        Utc.timestamp_opt(
                            t.floor() as i64,
                            (t.fract() * 1000.0 * 1000.0 * 1000.0) as u32,
                        )
                        .single()
                        .expect("invalid timestamp"),
                    );
                    Ok(())
                } else {
                    Err(ApiError::InvalidDataFormat {
                        event: self.envelopes_processed.saturating_sub(1),
                    }
                    .into())
                }
            }
            Some(None) => Err(ApiError::InvalidDataFormat {
                event: self.envelopes_processed.saturating_sub(1),
            }
            .into()),
        }
    }

    fn build_event(&mut self, mut json: JsonValue) -> Result<Event, Rejection> {
        self.envelopes_processed += 1;
        // Construct Event from parsed json event
        let mut log = match self.log_namespace {
            LogNamespace::Vector => self.build_log_vector(&mut json)?,
            LogNamespace::Legacy => self.build_log_legacy(&mut json)?,
        };

        // Add source type
        self.log_namespace.insert_vector_metadata(
            &mut log,
            log_schema().source_type_key(),
            &owned_value_path!("source_type"),
            SplunkConfig::NAME,
        );

        // Process channel field
        let channel_path = owned_value_path!(CHANNEL);
        if let Some(JsonValue::String(guid)) = json.get_mut("channel").map(JsonValue::take) {
            self.log_namespace.insert_source_metadata(
                SplunkConfig::NAME,
                &mut log,
                Some(LegacyKey::Overwrite(&channel_path)),
                lookup::path!(CHANNEL),
                guid,
            );
        } else if let Some(guid) = self.channel.as_ref() {
            self.log_namespace.insert_source_metadata(
                SplunkConfig::NAME,
                &mut log,
                Some(LegacyKey::Overwrite(&channel_path)),
                lookup::path!(CHANNEL),
                guid.clone(),
            );
        }

        // Process fields field
        if let Some(JsonValue::Object(object)) = json.get_mut("fields").map(JsonValue::take) {
            for (key, value) in object {
                self.log_namespace.insert_source_metadata(
                    SplunkConfig::NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(&owned_value_path!(key.as_str()))),
                    lookup::path!(key.as_str()),
                    value,
                );
            }
        }

        self.process_time(&mut json)?;

        // Add time field
        let timestamp = match self.time.clone() {
            Time::Provided(time) => time,
            Time::Now(time) => time,
        };

        self.log_namespace.insert_source_metadata(
            SplunkConfig::NAME,
            &mut log,
            log_schema().timestamp_key().map(LegacyKey::Overwrite),
            lookup::path!("timestamp"),
            timestamp,
        );

        // Extract default extracted fields
        for de in self.extractors.iter_mut() {
            de.extract(&mut log, &mut json);
        }

        // Add passthrough token if present
        if let Some(token) = &self.token {
            log.metadata_mut().set_splunk_hec_token(Arc::clone(token));
        }

        if let Some(batch) = self.batch.clone() {
            log = log.with_batch_notifier(&batch);
        }

        Ok(log.into())
    }

    /// Decoded path: extract the envelope's `event` field as bytes (preserving shape),
    /// run it through the second-stage decoder, and overlay envelope metadata so that
    /// decoder-produced fields win on conflict. Returns the events along with a flag
    /// indicating whether the codec hit any errors (so the caller can refuse to ack
    /// a request that lost data).
    fn build_events_decoded(
        &mut self,
        mut json: JsonValue,
        decoder: Decoder,
    ) -> Result<(Vec<Event>, bool), Rejection> {
        self.envelopes_processed += 1;
        // Extract `event` field while preserving its original JSON shape.
        // Strings go in as raw UTF-8 bytes (so non-JSON decoders see bare content);
        // objects, arrays, numbers, and bools are JSON-serialized so a JSON decoder
        // round-trips them back to the same shape.
        let payload: Vec<u8> = match json.get_mut("event").map(JsonValue::take) {
            Some(JsonValue::Null) | None => {
                return Err(ApiError::MissingEventField {
                    event: self.envelopes_processed.saturating_sub(1),
                }
                .into());
            }
            Some(JsonValue::String(s)) => s.into_bytes(),
            Some(other) => serde_json::to_vec(&other).map_err(|_| ApiError::InvalidDataFormat {
                event: self.envelopes_processed.saturating_sub(1),
            })?,
        };

        self.process_time(&mut json)?;

        // Always forward a fallback timestamp so events without an explicit envelope
        // `time` field still get one (matches the legacy /event behavior, which always
        // wrote a timestamp). `decode_message` uses `try_insert`, so a decoder-supplied
        // timestamp still wins on conflict.
        let fallback_time = match self.time {
            Time::Provided(t) | Time::Now(t) => t,
        };

        let (decoded, had_decode_errors) = decode_payload(
            decoder,
            &payload,
            Some(fallback_time),
            &self.batch,
            self.log_namespace,
            &self.events_received,
        );

        // Snapshot envelope metadata that has to apply uniformly to every decoded event.
        let envelope_channel: Option<Value> = match json.get_mut("channel").map(JsonValue::take) {
            Some(JsonValue::String(guid)) => Some(guid.into()),
            _ => None,
        };
        let envelope_fields: Option<serde_json::Map<String, JsonValue>> =
            match json.get_mut("fields").map(JsonValue::take) {
                Some(JsonValue::Object(object)) => Some(object),
                _ => None,
            };
        let channel_path = owned_value_path!(CHANNEL);

        let mut out = Vec::with_capacity(decoded.len());
        for mut event in decoded {
            if let Event::Log(log) = &mut event {
                // channel: envelope value beats header default. Use `InsertIfEmpty`
                // so a decoder-produced field at `splunk_channel` (legacy ns) wins on
                // conflict - matches the "decoder wins" rule applied to the rest of
                // the envelope metadata.
                if let Some(channel_val) = envelope_channel.clone().or_else(|| self.channel.clone())
                {
                    self.log_namespace.insert_source_metadata(
                        SplunkConfig::NAME,
                        log,
                        Some(LegacyKey::InsertIfEmpty(&channel_path)),
                        lookup::path!(CHANNEL),
                        channel_val,
                    );
                }

                // fields: legacy keys land at bare paths that may conflict with decoder
                // output, so use `InsertIfEmpty` to preserve decoder-wins semantics.
                if let Some(ref fields) = envelope_fields {
                    for (key, value) in fields {
                        self.log_namespace.insert_source_metadata(
                            SplunkConfig::NAME,
                            log,
                            Some(LegacyKey::InsertIfEmpty(&owned_value_path!(key.as_str()))),
                            lookup::path!(key.as_str()),
                            value.clone(),
                        );
                    }
                }

                // Default extractors (host/index/source/sourcetype). These take from
                // `json` on first call and stay sticky on subsequent calls within the
                // same envelope, so per-event invocation works correctly.
                for de in self.extractors.iter_mut() {
                    de.extract(log, &mut json);
                }
            }

            // The token lives on `EventMetadata`, which exists on every event type,
            // so apply it outside the `Event::Log` branch - codecs like `native` or
            // `otlp` can emit metrics or traces, and those still need the HEC token
            // for downstream Splunk HEC sinks.
            if let Some(token) = &self.token {
                event.metadata_mut().set_splunk_hec_token(Arc::clone(token));
            }
            out.push(event);
        }

        Ok((out, had_decode_errors))
    }

    /// Build the log event for the vector namespace.
    /// In this namespace the log event is created entirely from the event field.
    /// No renaming of the `line` field is done.
    fn build_log_vector(&mut self, json: &mut JsonValue) -> Result<LogEvent, Rejection> {
        match json.get("event") {
            Some(event) => {
                let event: Value = event.into();
                let mut log = LogEvent::from(event);

                // EstimatedJsonSizeOf must be calculated before enrichment
                self.events_received
                    .emit(CountByteSize(1, log.estimated_json_encoded_size_of()));

                // The timestamp is extracted from the message for the Legacy namespace.
                self.log_namespace.insert_vector_metadata(
                    &mut log,
                    log_schema().timestamp_key(),
                    lookup::path!("ingest_timestamp"),
                    chrono::Utc::now(),
                );

                Ok(log)
            }
            None => Err(ApiError::MissingEventField {
                event: self.envelopes_processed.saturating_sub(1),
            }
            .into()),
        }
    }

    /// Build the log event for the legacy namespace.
    /// If the event is a string, or the event contains a field called `line` that is a string
    /// (the docker splunk logger places the message in the event.line field) that string
    /// is placed in the message field.
    fn build_log_legacy(&mut self, json: &mut JsonValue) -> Result<LogEvent, Rejection> {
        let mut log = LogEvent::default();
        match json.get_mut("event") {
            Some(event) => match event.take() {
                JsonValue::String(string) => {
                    if string.is_empty() {
                        return Err(ApiError::EmptyEventField {
                            event: self.envelopes_processed.saturating_sub(1),
                        }
                        .into());
                    }
                    log.maybe_insert(log_schema().message_key_target_path(), string);
                }
                JsonValue::Object(mut object) => {
                    if object.is_empty() {
                        return Err(ApiError::EmptyEventField {
                            event: self.envelopes_processed.saturating_sub(1),
                        }
                        .into());
                    }

                    // Add 'line' value as 'event::schema().message_key'
                    if let Some(line) = object.remove("line") {
                        match line {
                            // This don't quite fit the meaning of a event::schema().message_key
                            JsonValue::Array(_) | JsonValue::Object(_) => {
                                log.insert(event_path!("line"), line);
                            }
                            _ => {
                                log.maybe_insert(log_schema().message_key_target_path(), line);
                            }
                        }
                    }

                    for (key, value) in object {
                        log.insert(event_path!(key.as_str()), value);
                    }
                }
                _ => {
                    return Err(ApiError::InvalidDataFormat {
                        event: self.envelopes_processed.saturating_sub(1),
                    }
                    .into());
                }
            },
            None => {
                return Err(ApiError::MissingEventField {
                    event: self.envelopes_processed.saturating_sub(1),
                }
                .into());
            }
        };

        // EstimatedJsonSizeOf must be calculated before enrichment
        self.events_received
            .emit(CountByteSize(1, log.estimated_json_encoded_size_of()));

        Ok(log)
    }
}

impl<'de, R: JsonRead<'de>> Iterator for EventIterator<'de, R> {
    /// Each item is `(events, had_decode_errors)` for one envelope - the boolean is
    /// only ever `true` in the decoder path. Callers OR these together across the
    /// whole request to decide whether ack registration is safe.
    type Item = Result<(Vec<Event>, bool), Rejection>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.deserializer.next() {
            Some(Ok(json)) => {
                let result = if let Some(decoder) = self.decoder.clone() {
                    self.build_events_decoded(json, decoder)
                } else {
                    self.build_event(json).map(|event| (vec![event], false))
                };
                Some(result)
            }
            None => {
                if self.envelopes_processed == 0 {
                    Some(Err(ApiError::NoData.into()))
                } else {
                    None
                }
            }
            Some(Err(error)) => {
                emit!(SplunkHecRequestBodyInvalidError {
                    error: error.into()
                });
                // The deserializer failed to parse the next envelope, so the failing
                // envelope's index is the count of envelopes already processed (not
                // `envelopes_processed - 1`, which is what build-time errors use).
                Some(Err(ApiError::InvalidDataFormat {
                    event: self.envelopes_processed,
                }
                .into()))
            }
        }
    }
}

/// Run a payload through the configured second-stage `framing` + `decoding` codec.
///
/// Returns the decoded events along with a flag indicating whether any decode error
/// occurred. The shared `crate::sources::util::decode_message` helper swallows
/// decode errors silently, which is fine for sources without ack semantics, but for
/// `splunk_hec` we need to know about errors so we can refuse to acknowledge a
/// request that lost data mid-stream.
///
/// On each decoded event this helper sets `source_type`, the `vector.ingest_timestamp`
/// metadata (Vector namespace), and a fallback timestamp via `try_insert` (so a
/// codec-supplied timestamp wins on conflict). It also attaches the per-request
/// `BatchNotifier` so acknowledgements flow correctly.
fn decode_payload(
    mut decoder: Decoder,
    payload: &[u8],
    fallback_timestamp: Option<DateTime<Utc>>,
    batch: &Option<BatchNotifier>,
    log_namespace: LogNamespace,
    events_received: &Registered<EventsReceived>,
) -> (Vec<Event>, bool) {
    let mut buffer = BytesMut::with_capacity(payload.len());
    buffer.extend_from_slice(payload);
    let now = Utc::now();
    let mut events: Vec<Event> = Vec::new();
    let mut had_errors = false;

    loop {
        match decoder.decode_eof(&mut buffer) {
            Ok(Some((decoded, _))) => {
                for mut event in decoded {
                    if let Event::Log(log) = &mut event {
                        log_namespace.insert_vector_metadata(
                            log,
                            log_schema().source_type_key(),
                            lookup::path!("source_type"),
                            Bytes::from_static(SplunkConfig::NAME.as_bytes()),
                        );
                        match log_namespace {
                            LogNamespace::Vector => {
                                if let Some(timestamp) = fallback_timestamp {
                                    log.try_insert(
                                        metadata_path!(SplunkConfig::NAME, "timestamp"),
                                        timestamp,
                                    );
                                }
                                log.insert(metadata_path!("vector", "ingest_timestamp"), now);
                            }
                            LogNamespace::Legacy => {
                                if let Some(timestamp) = fallback_timestamp
                                    && let Some(timestamp_key) = log_schema().timestamp_key()
                                {
                                    log.try_insert((PathPrefix::Event, timestamp_key), timestamp);
                                }
                            }
                        }
                    }
                    events_received.emit(CountByteSize(1, event.estimated_json_encoded_size_of()));
                    events.push(event.with_batch_notifier_option(batch));
                }
            }
            Ok(None) => break,
            Err(error) => {
                // The decoder logs its own error; record that one occurred so the
                // caller can refuse to ack a request that lost data.
                had_errors = true;
                if !error.can_continue() {
                    break;
                }
            }
        }
    }

    (events, had_errors)
}

/// Parse a `i64` unix timestamp that can either be in seconds, milliseconds or
/// nanoseconds.
///
/// This attempts to parse timestamps based on what cutoff range they fall into.
/// For seconds to be parsed the timestamp must be less than the unix epoch of
/// the year `2400`. For this to parse milliseconds the time must be smaller
/// than the year `10,000` in unix epoch milliseconds. If the value is larger
/// than both we attempt to parse it as nanoseconds.
///
/// Returns `None` if `t` is negative.
fn parse_timestamp(t: i64) -> Option<DateTime<Utc>> {
    // Utc.ymd(2400, 1, 1).and_hms(0,0,0).timestamp();
    const SEC_CUTOFF: i64 = 13569465600;
    // Utc.ymd(10_000, 1, 1).and_hms(0,0,0).timestamp_millis();
    const MILLISEC_CUTOFF: i64 = 253402300800000;

    // Timestamps can't be negative!
    if t < 0 {
        return None;
    }

    let ts = if t < SEC_CUTOFF {
        Utc.timestamp_opt(t, 0).single().expect("invalid timestamp")
    } else if t < MILLISEC_CUTOFF {
        Utc.timestamp_millis_opt(t)
            .single()
            .expect("invalid timestamp")
    } else {
        Utc.timestamp_nanos(t)
    };

    Some(ts)
}

/// How to write the legacy key when `DefaultExtractor::extract` applies a value.
#[derive(Clone, Copy)]
enum LegacyKeyStrategy {
    Overwrite,
    InsertIfEmpty,
}

/// Maintains last known extracted value of field and uses it in the absence of field.
struct DefaultExtractor {
    field: &'static str,
    to_field: OptionalValuePath,
    value: Option<Value>,
    log_namespace: LogNamespace,
    legacy_key_strategy: LegacyKeyStrategy,
}

impl DefaultExtractor {
    const fn new(
        field: &'static str,
        to_field: OptionalValuePath,
        log_namespace: LogNamespace,
    ) -> Self {
        DefaultExtractor {
            field,
            to_field,
            value: None,
            log_namespace,
            legacy_key_strategy: LegacyKeyStrategy::Overwrite,
        }
    }

    fn new_with(
        field: &'static str,
        to_field: OptionalValuePath,
        value: impl Into<Option<Value>>,
        log_namespace: LogNamespace,
    ) -> Self {
        DefaultExtractor {
            field,
            to_field,
            value: value.into(),
            log_namespace,
            legacy_key_strategy: LegacyKeyStrategy::Overwrite,
        }
    }

    /// Set the strategy used when writing this extractor's legacy key. Defaults to
    /// `Overwrite`; the decoder path uses `InsertIfEmpty` for fields that may collide
    /// with decoder-produced output (e.g. `host`).
    const fn with_legacy_key_strategy(mut self, strategy: LegacyKeyStrategy) -> Self {
        self.legacy_key_strategy = strategy;
        self
    }

    fn extract(&mut self, log: &mut LogEvent, value: &mut JsonValue) {
        // Process json_field
        if let Some(JsonValue::String(new_value)) = value.get_mut(self.field).map(JsonValue::take) {
            self.value = Some(new_value.into());
        }

        // Add data field
        if let Some(index) = self.value.as_ref()
            && let Some(metadata_key) = self.to_field.path.as_ref()
        {
            let legacy_key = match self.legacy_key_strategy {
                LegacyKeyStrategy::Overwrite => LegacyKey::Overwrite(metadata_key),
                LegacyKeyStrategy::InsertIfEmpty => LegacyKey::InsertIfEmpty(metadata_key),
            };
            self.log_namespace.insert_source_metadata(
                SplunkConfig::NAME,
                log,
                Some(legacy_key),
                &self.to_field.path.clone().unwrap_or(owned_value_path!("")),
                index.clone(),
            )
        }
    }
}

/// For tracking origin of the timestamp
#[derive(Clone, Debug)]
enum Time {
    /// Backup
    Now(DateTime<Utc>),
    /// Provided in the request
    Provided(DateTime<Utc>),
}

/// Creates events from a raw HEC request body.
///
/// Without a decoder, returns a single event whose message is the (decompressed)
/// request body. With a decoder, the body is fed through the configured framing +
/// decoding pipeline and one or more events are returned. The boolean second tuple
/// element is `true` when the decoder hit any (recoverable or non-recoverable)
/// errors during the request, so the caller can refuse to acknowledge the request.
#[allow(clippy::too_many_arguments)]
fn raw_event(
    bytes: Bytes,
    gzip: bool,
    channel: String,
    remote: Option<SocketAddr>,
    xff: Option<String>,
    batch: Option<BatchNotifier>,
    log_namespace: LogNamespace,
    events_received: &Registered<EventsReceived>,
    decoder: Option<Decoder>,
) -> Result<(Vec<Event>, bool), Rejection> {
    // Process gzip
    let body_bytes: Bytes = if gzip {
        let mut data = Vec::new();
        match MultiGzDecoder::new(bytes.reader()).read_to_end(&mut data) {
            Ok(0) => return Err(ApiError::NoData.into()),
            Ok(_) => Bytes::from(data),
            Err(error) => {
                emit!(SplunkHecRequestBodyInvalidError { error });
                return Err(ApiError::InvalidDataFormat { event: 0 }.into());
            }
        }
    } else {
        bytes
    };

    // host-field priority for raw endpoint:
    // - x-forwarded-for is set to `host` field first, if present. If not present:
    // - set remote addr to host field
    let host = if let Some(remote_address) = xff {
        Some(remote_address)
    } else {
        remote.map(|remote| remote.to_string())
    };

    let decoder_in_use = decoder.is_some();
    let (mut events, had_decode_errors): (Vec<Event>, bool) = if let Some(decoder) = decoder {
        // Pass ingest time as the fallback timestamp so decoded events always have
        // one - matches `insert_standard_vector_source_metadata` in the legacy raw
        // path. `decode_payload` uses `try_insert`, so a decoder-supplied timestamp
        // still wins on conflict.
        decode_payload(
            decoder,
            &body_bytes,
            Some(Utc::now()),
            &batch,
            log_namespace,
            events_received,
        )
    } else {
        let message: Value = body_bytes.into();
        let mut log = match log_namespace {
            LogNamespace::Vector => LogEvent::from(message),
            LogNamespace::Legacy => {
                let mut log = LogEvent::default();
                log.maybe_insert(log_schema().message_key_target_path(), message);
                log
            }
        };
        // We need to calculate the estimated json size of the event BEFORE enrichment.
        events_received.emit(CountByteSize(1, log.estimated_json_encoded_size_of()));

        log_namespace.insert_standard_vector_source_metadata(
            &mut log,
            SplunkConfig::NAME,
            Utc::now(),
        );

        if let Some(batch) = batch.clone() {
            log = log.with_batch_notifier(&batch);
        }
        (vec![Event::from(log)], false)
    };

    let channel_path = owned_value_path!(CHANNEL);
    for event in &mut events {
        if let Event::Log(log) = event {
            // With a decoder configured, defer to anything it produced at the legacy
            // `splunk_channel` path so "decoder wins" stays true. Without a decoder,
            // the log was just constructed and overwriting matches today's behavior.
            let channel_legacy_key = if decoder_in_use {
                LegacyKey::InsertIfEmpty(&channel_path)
            } else {
                LegacyKey::Overwrite(&channel_path)
            };
            log_namespace.insert_source_metadata(
                SplunkConfig::NAME,
                log,
                Some(channel_legacy_key),
                lookup::path!(CHANNEL),
                channel.clone(),
            );

            if let Some(ref host) = host {
                log_namespace.insert_source_metadata(
                    SplunkConfig::NAME,
                    log,
                    log_schema().host_key().map(LegacyKey::InsertIfEmpty),
                    lookup::path!("host"),
                    host.clone(),
                );
            }
        }
    }

    Ok((events, had_decode_errors))
}

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    MissingAuthorization,
    InvalidAuthorization,
    UnsupportedEncoding,
    UnsupportedContentType,
    MissingChannel,
    NoData,
    InvalidDataFormat { event: usize },
    ServerShutdown,
    EmptyEventField { event: usize },
    MissingEventField { event: usize },
    BadRequest,
    ServiceUnavailable,
    AckIsDisabled,
}

impl warp::reject::Reject for ApiError {}

/// Cached bodies for common responses
mod splunk_response {
    use serde::Serialize;

    // https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/TroubleshootHTTPEventCollector#Possible_error_codes
    pub enum HecStatusCode {
        Success = 0,
        TokenIsRequired = 2,
        InvalidAuthorization = 3,
        NoData = 5,
        InvalidDataFormat = 6,
        ServerIsBusy = 9,
        DataChannelIsMissing = 10,
        EventFieldIsRequired = 12,
        EventFieldCannotBeBlank = 13,
        AckIsDisabled = 14,
    }

    #[derive(Serialize)]
    pub enum HecResponseMetadata {
        #[serde(rename = "ackId")]
        AckId(u64),
        #[serde(rename = "invalid-event-number")]
        InvalidEventNumber(usize),
    }

    #[derive(Serialize)]
    pub struct HecResponse {
        text: &'static str,
        code: u8,
        #[serde(skip_serializing_if = "Option::is_none", flatten)]
        pub metadata: Option<HecResponseMetadata>,
    }

    impl HecResponse {
        pub const fn new(code: HecStatusCode) -> Self {
            let text = match code {
                HecStatusCode::Success => "Success",
                HecStatusCode::TokenIsRequired => "Token is required",
                HecStatusCode::InvalidAuthorization => "Invalid authorization",
                HecStatusCode::NoData => "No data",
                HecStatusCode::InvalidDataFormat => "Invalid data format",
                HecStatusCode::DataChannelIsMissing => "Data channel is missing",
                HecStatusCode::EventFieldIsRequired => "Event field is required",
                HecStatusCode::EventFieldCannotBeBlank => "Event field cannot be blank",
                HecStatusCode::ServerIsBusy => "Server is busy",
                HecStatusCode::AckIsDisabled => "Ack is disabled",
            };

            Self {
                text,
                code: code as u8,
                metadata: None,
            }
        }

        pub const fn with_metadata(mut self, metadata: HecResponseMetadata) -> Self {
            self.metadata = Some(metadata);
            self
        }
    }

    pub const INVALID_AUTHORIZATION: HecResponse =
        HecResponse::new(HecStatusCode::InvalidAuthorization);
    pub const TOKEN_IS_REQUIRED: HecResponse = HecResponse::new(HecStatusCode::TokenIsRequired);
    pub const NO_DATA: HecResponse = HecResponse::new(HecStatusCode::NoData);
    pub const SUCCESS: HecResponse = HecResponse::new(HecStatusCode::Success);
    pub const SERVER_IS_BUSY: HecResponse = HecResponse::new(HecStatusCode::ServerIsBusy);
    pub const NO_CHANNEL: HecResponse = HecResponse::new(HecStatusCode::DataChannelIsMissing);
    pub const ACK_IS_DISABLED: HecResponse = HecResponse::new(HecStatusCode::AckIsDisabled);
}

fn finish_ok(maybe_ack_id: Option<u64>) -> Response {
    let body = if let Some(ack_id) = maybe_ack_id {
        HecResponse::new(HecStatusCode::Success).with_metadata(HecResponseMetadata::AckId(ack_id))
    } else {
        splunk_response::SUCCESS
    };
    response_json(StatusCode::OK, body)
}

fn response_plain(code: StatusCode, msg: &'static str) -> Response {
    warp::reply::with_status(
        warp::reply::with_header(msg, http::header::CONTENT_TYPE, "text/plain; charset=utf-8"),
        code,
    )
    .into_response()
}

async fn finish_err(rejection: Rejection) -> Result<(Response,), Rejection> {
    if let Some(&error) = rejection.find::<ApiError>() {
        emit!(SplunkHecRequestError { error });
        Ok((match error {
            ApiError::MissingAuthorization => {
                response_json(StatusCode::UNAUTHORIZED, splunk_response::TOKEN_IS_REQUIRED)
            }
            ApiError::InvalidAuthorization => response_json(
                StatusCode::UNAUTHORIZED,
                splunk_response::INVALID_AUTHORIZATION,
            ),
            ApiError::UnsupportedEncoding => empty_response(StatusCode::UNSUPPORTED_MEDIA_TYPE),
            ApiError::UnsupportedContentType => response_plain(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "The request's content-type is not supported",
            ),
            ApiError::MissingChannel => {
                response_json(StatusCode::BAD_REQUEST, splunk_response::NO_CHANNEL)
            }
            ApiError::NoData => response_json(StatusCode::BAD_REQUEST, splunk_response::NO_DATA),
            ApiError::ServerShutdown => empty_response(StatusCode::SERVICE_UNAVAILABLE),
            ApiError::InvalidDataFormat { event } => response_json(
                StatusCode::BAD_REQUEST,
                HecResponse::new(HecStatusCode::InvalidDataFormat)
                    .with_metadata(HecResponseMetadata::InvalidEventNumber(event)),
            ),
            ApiError::EmptyEventField { event } => response_json(
                StatusCode::BAD_REQUEST,
                HecResponse::new(HecStatusCode::EventFieldCannotBeBlank)
                    .with_metadata(HecResponseMetadata::InvalidEventNumber(event)),
            ),
            ApiError::MissingEventField { event } => response_json(
                StatusCode::BAD_REQUEST,
                HecResponse::new(HecStatusCode::EventFieldIsRequired)
                    .with_metadata(HecResponseMetadata::InvalidEventNumber(event)),
            ),
            ApiError::BadRequest => empty_response(StatusCode::BAD_REQUEST),
            ApiError::ServiceUnavailable => response_json(
                StatusCode::SERVICE_UNAVAILABLE,
                splunk_response::SERVER_IS_BUSY,
            ),
            ApiError::AckIsDisabled => {
                response_json(StatusCode::BAD_REQUEST, splunk_response::ACK_IS_DISABLED)
            }
        },))
    } else {
        Err(rejection)
    }
}

/// Response without body
fn empty_response(code: StatusCode) -> Response {
    let mut res = Response::default();
    *res.status_mut() = code;
    res
}

/// Response with body
fn response_json(code: StatusCode, body: impl Serialize) -> Response {
    warp::reply::with_status(warp::reply::json(&body), code).into_response()
}

#[cfg(feature = "sinks-splunk_hec")]
#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, num::NonZeroU64};

    use chrono::{TimeZone, Utc};
    use futures_util::Stream;
    use http::Uri;
    use reqwest::{RequestBuilder, Response};
    use serde::Deserialize;
    use vector_lib::{
        codecs::{
            BytesDecoderConfig, JsonSerializerConfig, TextSerializerConfig,
            decoding::DeserializerConfig,
        },
        event::EventStatus,
        schema::Definition,
        sensitive_string::SensitiveString,
    };
    use vrl::path::PathPrefix;

    use super::*;
    use crate::{
        SourceSender,
        codecs::{DecodingConfig, EncodingConfig},
        components::validation::prelude::*,
        config::{SinkConfig, SinkContext, SourceConfig, SourceContext, log_schema},
        event::{Event, LogEvent},
        sinks::{
            Healthcheck, VectorSink,
            splunk_hec::logs::config::HecLogsSinkConfig,
            util::{BatchConfig, Compression, TowerRequestConfig},
        },
        sources::splunk_hec::acknowledgements::{HecAckStatusRequest, HecAckStatusResponse},
        test_util::{
            addr::{PortGuard, next_addr},
            collect_n,
            components::{
                COMPONENT_ERROR_TAGS, HTTP_PUSH_SOURCE_TAGS, assert_source_compliance,
                assert_source_error,
            },
            wait_for_tcp,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SplunkConfig>();
    }

    /// Splunk token
    const TOKEN: &str = "token";
    const VALID_TOKENS: &[&str; 2] = &[TOKEN, "secondary-token"];

    async fn source(
        acknowledgements: Option<HecAcknowledgementsConfig>,
    ) -> (impl Stream<Item = Event> + Unpin, SocketAddr, PortGuard) {
        source_with(Some(TOKEN.to_owned().into()), None, acknowledgements, false).await
    }

    async fn source_with(
        token: Option<SensitiveString>,
        valid_tokens: Option<&[&str]>,
        acknowledgements: Option<HecAcknowledgementsConfig>,
        store_hec_token: bool,
    ) -> (
        impl Stream<Item = Event> + Unpin + use<>,
        SocketAddr,
        PortGuard,
    ) {
        let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let (_guard, address) = next_addr();
        let valid_tokens =
            valid_tokens.map(|tokens| tokens.iter().map(|v| v.to_string().into()).collect());
        let cx = SourceContext::new_test(sender, None);
        tokio::spawn(async move {
            SplunkConfig {
                address,
                token,
                valid_tokens,
                tls: None,
                acknowledgements: acknowledgements.unwrap_or_default(),
                store_hec_token,
                log_namespace: None,
                keepalive: Default::default(),
                event: EndpointCodecConfig::default(),
                raw: EndpointCodecConfig::default(),
            }
            .build(cx)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address, _guard)
    }

    async fn sink(
        address: SocketAddr,
        encoding: EncodingConfig,
        compression: Compression,
    ) -> (VectorSink, Healthcheck) {
        HecLogsSinkConfig {
            default_token: TOKEN.to_owned().into(),
            endpoint: format!("http://{address}"),
            host_key: None,
            indexed_fields: vec![],
            index: None,
            sourcetype: None,
            source: None,
            encoding,
            compression,
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
            acknowledgements: Default::default(),
            timestamp_nanos_key: None,
            timestamp_key: None,
            auto_extract_timestamp: None,
            endpoint_target: Default::default(),
        }
        .build(SinkContext::default())
        .await
        .unwrap()
    }

    async fn start(
        encoding: EncodingConfig,
        compression: Compression,
        acknowledgements: Option<HecAcknowledgementsConfig>,
    ) -> (VectorSink, impl Stream<Item = Event> + Unpin) {
        let (source, address, _guard) = source(acknowledgements).await;
        let (sink, health) = sink(address, encoding, compression).await;
        assert!(health.await.is_ok());
        (sink, source)
    }

    async fn channel_n(
        messages: Vec<impl Into<String> + Send + 'static>,
        sink: VectorSink,
        source: impl Stream<Item = Event> + Unpin,
    ) -> Vec<Event> {
        let n = messages.len();

        tokio::spawn(async move {
            sink.run_events(
                messages
                    .into_iter()
                    .map(|s| Event::Log(LogEvent::from(s.into()))),
            )
            .await
            .unwrap();
        });

        let events = collect_n(source, n).await;
        assert_eq!(n, events.len());

        events
    }

    #[derive(Clone, Copy, Debug)]
    enum Channel<'a> {
        Header(&'a str),
        QueryParam(&'a str),
    }

    #[derive(Default)]
    struct SendWithOpts<'a> {
        channel: Option<Channel<'a>>,
        forwarded_for: Option<String>,
    }

    async fn post(address: SocketAddr, api: &str, message: &str) -> u16 {
        let channel = Channel::Header("channel");
        let options = SendWithOpts {
            channel: Some(channel),
            forwarded_for: None,
        };
        send_with(address, api, message, TOKEN, &options).await
    }

    fn build_request(
        address: SocketAddr,
        api: &str,
        message: &str,
        token: &str,
        opts: &SendWithOpts<'_>,
    ) -> RequestBuilder {
        let mut b = reqwest::Client::new()
            .post(format!("http://{address}/{api}"))
            .header("Authorization", format!("Splunk {token}"));

        b = match opts.channel {
            Some(c) => match c {
                Channel::Header(v) => b.header("x-splunk-request-channel", v),
                Channel::QueryParam(v) => b.query(&[("channel", v)]),
            },
            None => b,
        };

        b = match &opts.forwarded_for {
            Some(f) => b.header("X-Forwarded-For", f),
            None => b,
        };

        b.body(message.to_owned())
    }

    async fn send_with(
        address: SocketAddr,
        api: &str,
        message: &str,
        token: &str,
        opts: &SendWithOpts<'_>,
    ) -> u16 {
        let b = build_request(address, api, message, token, opts);
        b.send().await.unwrap().status().as_u16()
    }

    async fn send_with_response(
        address: SocketAddr,
        api: &str,
        message: &str,
        token: &str,
        opts: &SendWithOpts<'_>,
    ) -> Response {
        let b = build_request(address, api, message, token, opts);
        b.send().await.unwrap()
    }

    #[tokio::test]
    async fn no_compression_text_event() {
        let message = "gzip_text_event";
        let (sink, source) = start(
            TextSerializerConfig::default().into(),
            Compression::None,
            None,
        )
        .await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn one_simple_text_event() {
        let message = "one_simple_text_event";
        let (sink, source) = start(
            TextSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn multiple_simple_text_event() {
        let n = 200;
        let (sink, source) = start(
            TextSerializerConfig::default().into(),
            Compression::None,
            None,
        )
        .await;

        let messages = (0..n)
            .map(|i| format!("multiple_simple_text_event_{i}"))
            .collect::<Vec<_>>();
        let events = channel_n(messages.clone(), sink, source).await;

        for (msg, event) in messages.into_iter().zip(events.into_iter()) {
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                msg.into()
            );
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        }
    }

    #[tokio::test]
    async fn one_simple_json_event() {
        let message = "one_simple_json_event";
        let (sink, source) = start(
            JsonSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn multiple_simple_json_event() {
        let n = 200;
        let (sink, source) = start(
            JsonSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let messages = (0..n)
            .map(|i| format!("multiple_simple_json_event{i}"))
            .collect::<Vec<_>>();
        let events = channel_n(messages.clone(), sink, source).await;

        for (msg, event) in messages.into_iter().zip(events.into_iter()) {
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                msg.into()
            );
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        }
    }

    #[tokio::test]
    async fn json_event() {
        let (sink, source) = start(
            JsonSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let mut log = LogEvent::default();
        log.insert("greeting", "hello");
        log.insert("name", "bob");
        sink.run_events(vec![log.into()]).await.unwrap();

        let event = collect_n(source, 1).await.remove(0).into_log();
        assert_eq!(event["greeting"], "hello".into());
        assert_eq!(event["name"], "bob".into());
        assert!(event.get_timestamp().is_some());
        assert_eq!(
            event[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn json_invalid_path_event() {
        let (sink, source) = start(
            JsonSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let mut log = LogEvent::default();
        // Test with a field that would be considered an invalid path if it were to
        // be treated as a path and not a simple field name.
        log.insert(event_path!("(greeting | thing"), "hello");
        sink.run_events(vec![log.into()]).await.unwrap();

        let event = collect_n(source, 1).await.remove(0).into_log();
        assert_eq!(
            event.get(event_path!("(greeting | thing")),
            Some(&Value::from("hello"))
        );
    }

    #[tokio::test]
    async fn line_to_message() {
        let (sink, source) = start(
            JsonSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let mut event = LogEvent::default();
        event.insert("line", "hello");
        sink.run_events(vec![event.into()]).await.unwrap();

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            "hello".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn raw() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "raw";
            let (source, address, _guard) = source(None).await;

            assert_eq!(200, post(address, "services/collector/raw", message).await);

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
            assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        })
        .await;
    }

    #[tokio::test]
    async fn root() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#"{ "event": { "message": "root"} }"#;
            let (source, address, _guard) = source(None).await;

            assert_eq!(200, post(address, "services/collector", message).await);

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "root".into()
            );
            assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        })
        .await;
    }

    #[tokio::test]
    async fn channel_header() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "raw";
            let (source, address, _guard) = source(None).await;

            let opts = SendWithOpts {
                channel: Some(Channel::Header("guid")),
                forwarded_for: None,
            };

            assert_eq!(
                200,
                send_with(address, "services/collector/raw", message, TOKEN, &opts).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
        })
        .await;
    }

    #[tokio::test]
    async fn xff_header_raw() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "raw";
            let (source, address, _guard) = source(None).await;

            let opts = SendWithOpts {
                channel: Some(Channel::Header("guid")),
                forwarded_for: Some(String::from("10.0.0.1")),
            };

            assert_eq!(
                200,
                send_with(address, "services/collector/raw", message, TOKEN, &opts).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().host_key().unwrap().to_string().as_str()],
                "10.0.0.1".into()
            );
        })
        .await;
    }

    // Test helps to illustrate that a payload's `host` value should override an x-forwarded-for header
    #[tokio::test]
    async fn xff_header_event_with_host_field() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#"{"event":"first", "host": "10.1.0.2"}"#;
            let (source, address, _guard) = source(None).await;

            let opts = SendWithOpts {
                channel: Some(Channel::Header("guid")),
                forwarded_for: Some(String::from("10.0.0.1")),
            };

            assert_eq!(
                200,
                send_with(address, "services/collector/event", message, TOKEN, &opts).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().host_key().unwrap().to_string().as_str()],
                "10.1.0.2".into()
            );
        })
        .await;
    }

    // Test helps to illustrate that a payload's `host` value should override an x-forwarded-for header
    #[tokio::test]
    async fn xff_header_event_without_host_field() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#"{"event":"first", "color": "blue"}"#;
            let (source, address, _guard) = source(None).await;

            let opts = SendWithOpts {
                channel: Some(Channel::Header("guid")),
                forwarded_for: Some(String::from("10.0.0.1")),
            };

            assert_eq!(
                200,
                send_with(address, "services/collector/event", message, TOKEN, &opts).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().host_key().unwrap().to_string().as_str()],
                "10.0.0.1".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn channel_query_param() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "raw";
            let (source, address, _guard) = source(None).await;

            let opts = SendWithOpts {
                channel: Some(Channel::QueryParam("guid")),
                forwarded_for: None,
            };

            assert_eq!(
                200,
                send_with(address, "services/collector/raw", message, TOKEN, &opts).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
        })
        .await;
    }

    #[tokio::test]
    async fn no_data() {
        let (_source, address, _guard) = source(None).await;

        assert_eq!(400, post(address, "services/collector/event", "").await);
    }

    #[tokio::test]
    async fn invalid_token() {
        assert_source_error(&COMPONENT_ERROR_TAGS, async {
            let (_source, address, _guard) = source(None).await;
            let opts = SendWithOpts {
                channel: Some(Channel::Header("channel")),
                forwarded_for: None,
            };

            assert_eq!(
                401,
                send_with(address, "services/collector/event", "", "nope", &opts).await
            );
        })
        .await;
    }

    #[tokio::test]
    async fn health_ignores_token() {
        let (_source, address, _guard) = source(None).await;

        let res = reqwest::Client::new()
            .get(format!("http://{address}/services/collector/health"))
            .header("Authorization", format!("Splunk {}", "invalid token"))
            .send()
            .await
            .unwrap();

        assert_eq!(200, res.status().as_u16());
    }

    #[tokio::test]
    async fn health() {
        let (_source, address, _guard) = source(None).await;

        let res = reqwest::Client::new()
            .get(format!("http://{address}/services/collector/health"))
            .send()
            .await
            .unwrap();

        assert_eq!(200, res.status().as_u16());
    }

    #[tokio::test]
    async fn secondary_token() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#"{"event":"first", "color": "blue"}"#;
            let (_source, address, _guard) =
                source_with(None, Some(VALID_TOKENS), None, false).await;
            let options = SendWithOpts {
                channel: None,
                forwarded_for: None,
            };

            assert_eq!(
                200,
                send_with(
                    address,
                    "services/collector/event",
                    message,
                    VALID_TOKENS.get(1).unwrap(),
                    &options
                )
                .await
            );
        })
        .await;
    }

    #[tokio::test]
    async fn event_service_token_passthrough_enabled() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "passthrough_token_enabled";
            let (source, address, _guard) = source_with(None, Some(VALID_TOKENS), None, true).await;
            let (sink, health) = sink(
                address,
                TextSerializerConfig::default().into(),
                Compression::gzip_default(),
            )
            .await;
            assert!(health.await.is_ok());

            let event = channel_n(vec![message], sink, source).await.remove(0);

            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
            assert_eq!(
                &event.metadata().splunk_hec_token().as_ref().unwrap()[..],
                TOKEN
            );
        })
        .await;
    }

    #[tokio::test]
    async fn raw_service_token_passthrough_enabled() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "raw";
            let (source, address, _guard) = source_with(None, Some(VALID_TOKENS), None, true).await;

            assert_eq!(200, post(address, "services/collector/raw", message).await);

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
            assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
            assert_eq!(
                &event.metadata().splunk_hec_token().as_ref().unwrap()[..],
                TOKEN
            );
        })
        .await;
    }

    #[tokio::test]
    async fn no_authorization() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "no_authorization";
            let (source, address, _guard) = source_with(None, None, None, false).await;
            let (sink, health) = sink(
                address,
                TextSerializerConfig::default().into(),
                Compression::gzip_default(),
            )
            .await;
            assert!(health.await.is_ok());

            let event = channel_n(vec![message], sink, source).await.remove(0);

            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        })
        .await;
    }

    #[tokio::test]
    async fn no_authorization_token_passthrough_enabled() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "no_authorization";
            let (source, address, _guard) = source_with(None, None, None, true).await;
            let (sink, health) = sink(
                address,
                TextSerializerConfig::default().into(),
                Compression::gzip_default(),
            )
            .await;
            assert!(health.await.is_ok());

            let event = channel_n(vec![message], sink, source).await.remove(0);

            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
            assert_eq!(
                &event.metadata().splunk_hec_token().as_ref().unwrap()[..],
                TOKEN
            );
        })
        .await;
    }

    #[tokio::test]
    async fn partial() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#"{"event":"first"}{"event":"second""#;
            let (source, address, _guard) = source(None).await;

            assert_eq!(
                400,
                post(address, "services/collector/event", message).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "first".into()
            );
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn handles_newlines() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#"
{"event":"first"}
        "#;
            let (source, address, _guard) = source(None).await;

            assert_eq!(
                200,
                post(address, "services/collector/event", message).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "first".into()
            );
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn handles_spaces() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = r#" {"event":"first"} "#;
            let (source, address, _guard) = source(None).await;

            assert_eq!(
                200,
                post(address, "services/collector/event", message).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                "first".into()
            );
            assert!(event.as_log().get_timestamp().is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                "splunk_hec".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn handles_non_utf8() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = b" {\"event\": { \"non\": \"A non UTF8 character \xE4\", \"number\": 2, \"bool\": true } } ";
        let (source, address, _guard) = source(None).await;

        let b = reqwest::Client::new()
            .post(format!(
                "http://{}/{}",
                address, "services/collector/event"
            ))
            .header("Authorization", format!("Splunk {TOKEN}"))
            .body::<&[u8]>(message);

        assert_eq!(200, b.send().await.unwrap().status().as_u16());

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(event.as_log()["non"], "A non UTF8 character �".into());
        assert_eq!(event.as_log()["number"], 2.into());
        assert_eq!(event.as_log()["bool"], true.into());
        assert!(event.as_log().get((lookup::PathPrefix::Event, log_schema().timestamp_key().unwrap())).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
    }).await;
    }

    #[tokio::test]
    async fn default() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"{"event":"first","source":"main"}{"event":"second"}{"event":"third","source":"secondary"}"#;
        let (source, address, _guard) = source(None).await;

        assert_eq!(
            200,
            post(address, "services/collector/event", message).await
        );

        let events = collect_n(source, 3).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "first".into()
        );
        assert_eq!(events[0].as_log()[&super::SOURCE], "main".into());

        assert_eq!(
            events[1].as_log()[log_schema().message_key().unwrap().to_string()],
            "second".into()
        );
        assert_eq!(events[1].as_log()[&super::SOURCE], "main".into());

        assert_eq!(
            events[2].as_log()[log_schema().message_key().unwrap().to_string()],
            "third".into()
        );
        assert_eq!(events[2].as_log()[&super::SOURCE], "secondary".into());
    }).await;
    }

    #[test]
    fn parse_timestamps() {
        let cases = vec![
            Utc::now(),
            Utc.with_ymd_and_hms(1971, 11, 7, 1, 1, 1)
                .single()
                .expect("invalid timestamp"),
            Utc.with_ymd_and_hms(2011, 8, 5, 1, 1, 1)
                .single()
                .expect("invalid timestamp"),
            Utc.with_ymd_and_hms(2189, 11, 4, 2, 2, 2)
                .single()
                .expect("invalid timestamp"),
        ];

        for case in cases {
            let sec = case.timestamp();
            let millis = case.timestamp_millis();
            let nano = case.timestamp_nanos_opt().expect("Timestamp out of range");

            assert_eq!(parse_timestamp(sec).unwrap().timestamp(), case.timestamp());
            assert_eq!(
                parse_timestamp(millis).unwrap().timestamp_millis(),
                case.timestamp_millis()
            );
            assert_eq!(
                parse_timestamp(nano)
                    .unwrap()
                    .timestamp_nanos_opt()
                    .unwrap(),
                case.timestamp_nanos_opt().expect("Timestamp out of range")
            );
        }

        assert!(parse_timestamp(-1).is_none());
    }

    /// This test will fail once `warp` crate fixes support for
    /// custom connection listener, at that point this test can be
    /// modified to pass.
    /// https://github.com/vectordotdev/vector/issues/7097
    /// https://github.com/seanmonstar/warp/issues/830
    /// https://github.com/seanmonstar/warp/pull/713
    #[tokio::test]
    async fn host_test() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let message = "for the host";
            let (sink, source) = start(
                TextSerializerConfig::default().into(),
                Compression::gzip_default(),
                None,
            )
            .await;

            let event = channel_n(vec![message], sink, source).await.remove(0);

            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
            assert!(
                event
                    .as_log()
                    .get((PathPrefix::Event, log_schema().host_key().unwrap()))
                    .is_none()
            );
        })
        .await;
    }

    #[derive(Deserialize)]
    struct HecAckEventResponse {
        text: String,
        code: u8,
        #[serde(rename = "ackId")]
        ack_id: u64,
    }

    #[tokio::test]
    async fn ack_json_event() {
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (source, address, _guard) = source(Some(ack_config)).await;
        let event_message = r#"{"event":"first", "color": "blue"}{"event":"second"}"#;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let event_res = send_with_response(
            address,
            "services/collector/event",
            event_message,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        assert_eq!("Success", event_res.text.as_str());
        assert_eq!(0, event_res.code);
        _ = collect_n(source, 1).await;

        let ack_message = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_raw_event() {
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (source, address, _guard) = source(Some(ack_config)).await;
        let event_message = "raw event message";
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let event_res = send_with_response(
            address,
            "services/collector/raw",
            event_message,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        assert_eq!("Success", event_res.text.as_str());
        assert_eq!(0, event_res.code);
        _ = collect_n(source, 1).await;

        let ack_message = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_repeat_ack_query() {
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (source, address, _guard) = source(Some(ack_config)).await;
        let event_message = "raw event message";
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let event_res = send_with_response(
            address,
            "services/collector/raw",
            event_message,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        _ = collect_n(source, 1).await;

        let ack_message = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());

        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(!ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_exceed_max_number_of_ack_channels() {
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };

        let (_source, address, _guard) = source(Some(ack_config)).await;
        let mut opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        assert_eq!(
            200,
            send_with(address, "services/collector/raw", "message", TOKEN, &opts).await
        );

        opts.channel = Some(Channel::Header("other-guid"));
        assert_eq!(
            503,
            send_with(address, "services/collector/raw", "message", TOKEN, &opts).await
        );
        assert_eq!(
            503,
            send_with(
                address,
                "services/collector/event",
                r#"{"event":"first"}"#,
                TOKEN,
                &opts
            )
            .await
        );
    }

    #[tokio::test]
    async fn ack_exceed_max_pending_acks_per_channel() {
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            max_pending_acks_per_channel: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };

        let (source, address, _guard) = source(Some(ack_config)).await;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        for _ in 0..5 {
            send_with(
                address,
                "services/collector/event",
                r#"{"event":"first"}"#,
                TOKEN,
                &opts,
            )
            .await;
        }
        for _ in 0..5 {
            send_with(address, "services/collector/raw", "message", TOKEN, &opts).await;
        }
        let event_res = send_with_response(
            address,
            "services/collector/event",
            r#"{"event":"this will be acked"}"#,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        _ = collect_n(source, 11).await;

        let ack_message_dropped = serde_json::to_string(&HecAckStatusRequest {
            acks: (0..10).collect::<Vec<u64>>(),
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message_dropped.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.values().all(|ack_status| !*ack_status));

        let ack_message_acked = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message_acked.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_service_accepts_parameterized_content_type() {
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (source, address, _guard) = source(Some(ack_config)).await;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };

        let event_res = send_with_response(
            address,
            "services/collector/event",
            r#"{"event":"param-test"}"#,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        let _ = collect_n(source, 1).await;

        let body = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();

        let res = reqwest::Client::new()
            .post(format!("http://{address}/services/collector/ack"))
            .header("Authorization", format!("Splunk {TOKEN}"))
            .header("x-splunk-request-channel", "guid")
            .header("Content-Type", "application/json; some-random-text; hello")
            .body(body)
            .send()
            .await
            .unwrap();

        assert_eq!(200, res.status().as_u16());

        let _parsed: HecAckStatusResponse = res.json().await.unwrap();
    }

    #[tokio::test]
    async fn event_service_acknowledgements_enabled_channel_required() {
        let message = r#"{"event":"first", "color": "blue"}"#;
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (_, address, _guard) = source(Some(ack_config)).await;

        let opts = SendWithOpts {
            channel: None,
            forwarded_for: None,
        };

        assert_eq!(
            400,
            send_with(address, "services/collector/event", message, TOKEN, &opts).await
        );
    }

    #[tokio::test]
    async fn ack_service_acknowledgements_disabled() {
        let message = r#" {"acks":[0]} "#;
        let (_, address, _guard) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };

        assert_eq!(
            400,
            send_with(address, "services/collector/ack", message, TOKEN, &opts).await
        );
    }

    async fn source_with_codec(
        event: EndpointCodecConfig,
        raw: EndpointCodecConfig,
    ) -> (
        impl Stream<Item = Event> + Unpin + use<>,
        SocketAddr,
        PortGuard,
    ) {
        let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let (_guard, address) = next_addr();
        let cx = SourceContext::new_test(sender, None);
        tokio::spawn(async move {
            SplunkConfig {
                address,
                token: Some(TOKEN.to_owned().into()),
                valid_tokens: None,
                tls: None,
                acknowledgements: Default::default(),
                store_hec_token: false,
                log_namespace: None,
                keepalive: Default::default(),
                event,
                raw,
            }
            .build(cx)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address, _guard)
    }

    /// Codec config that just sets `decoding` (default framing).
    fn codec_decoding(decoding: DeserializerConfig) -> EndpointCodecConfig {
        EndpointCodecConfig {
            framing: None,
            decoding: Some(decoding),
        }
    }

    /// Codec config that sets both `framing` and `decoding`.
    fn codec_full(
        framing: Option<FramingConfig>,
        decoding: Option<DeserializerConfig>,
    ) -> EndpointCodecConfig {
        EndpointCodecConfig { framing, decoding }
    }

    #[tokio::test]
    async fn decoder_event_endpoint_json_string() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                EndpointCodecConfig::default(),
            )
            .await;
            let envelope =
                r#"{"event":"{\"foo\":\"bar\",\"n\":42}","host":"client-host","sourcetype":"my-app"}"#;
            assert_eq!(
                200,
                post(address, "services/collector/event", envelope).await
            );

            let event = collect_n(source, 1).await.remove(0);
            let log = event.as_log();
            assert_eq!(log["foo"], "bar".into());
            assert_eq!(log["n"], 42.into());
            assert_eq!(
                log[log_schema().host_key().unwrap().to_string().as_str()],
                "client-host".into()
            );
            assert_eq!(log[&super::SOURCETYPE], "my-app".into());
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_event_endpoint_json_object_round_trip() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                EndpointCodecConfig::default(),
            )
            .await;
            let envelope = r#"{"event":{"foo":"bar","nested":{"k":1}},"host":"h"}"#;
            assert_eq!(
                200,
                post(address, "services/collector/event", envelope).await
            );

            let event = collect_n(source, 1).await.remove(0);
            let log = event.as_log();
            assert_eq!(log["foo"], "bar".into());
            assert_eq!(*log.get("nested.k").unwrap(), 1.into());
            assert_eq!(
                log[log_schema().host_key().unwrap().to_string().as_str()],
                "h".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_event_endpoint_all_envelope_fields_yield_to_decoder() {
        // The decoded path must defer to the codec for `splunk_channel`,
        // `splunk_index`, `splunk_source`, and `splunk_sourcetype` in legacy ns -
        // not just `host`. Otherwise the changelog's "decoder wins on conflict"
        // promise is broken for HEC envelope metadata.
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                EndpointCodecConfig::default(),
            )
            .await;
            // The string `event` decodes to a JSON object that pre-populates each
            // legacy splunk_* field. The envelope sets conflicting values for the
            // same fields and must lose.
            let envelope = r#"{
                "event":"{\"splunk_channel\":\"decoder-channel\",\"splunk_index\":\"decoder-index\",\"splunk_source\":\"decoder-source\",\"splunk_sourcetype\":\"decoder-sourcetype\"}",
                "index":"envelope-index",
                "source":"envelope-source",
                "sourcetype":"envelope-sourcetype"
            }"#;
            assert_eq!(
                200,
                post(address, "services/collector/event", envelope).await
            );

            let event = collect_n(source, 1).await.remove(0);
            let log = event.as_log();
            assert_eq!(log[&super::CHANNEL], "decoder-channel".into());
            assert_eq!(log[&super::INDEX], "decoder-index".into());
            assert_eq!(log[&super::SOURCE], "decoder-source".into());
            assert_eq!(log[&super::SOURCETYPE], "decoder-sourcetype".into());
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_event_endpoint_decoder_field_wins_over_envelope() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                EndpointCodecConfig::default(),
            )
            .await;
            // The string `event` decodes to {host: "decoder-host"}; the envelope sets
            // host: "envelope-host". The decoder's value must win.
            let envelope = r#"{"event":"{\"host\":\"decoder-host\"}","host":"envelope-host"}"#;
            assert_eq!(
                200,
                post(address, "services/collector/event", envelope).await
            );

            let event = collect_n(source, 1).await.remove(0);
            let log = event.as_log();
            assert_eq!(
                log[log_schema().host_key().unwrap().to_string().as_str()],
                "decoder-host".into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_event_endpoint_decode_failure_returns_200() {
        // A malformed inner JSON must not surface as an HTTP error to the Splunk
        // client - decode failures are swallowed by the codec like other Vector
        // sources do.
        let (_source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            EndpointCodecConfig::default(),
        )
        .await;
        let envelope = r#"{"event":"not valid json {","host":"h"}"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", envelope).await
        );
    }

    #[tokio::test]
    async fn decoder_raw_endpoint_newline_delimited() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                EndpointCodecConfig::default(),
                codec_full(
                    Some(FramingConfig::NewlineDelimited(Default::default())),
                    Some(DeserializerConfig::Bytes),
                ),
            )
            .await;
            let body = "line1\nline2\nline3";
            assert_eq!(200, post(address, "services/collector/raw", body).await);

            let events = collect_n(source, 3).await;
            assert_eq!(events.len(), 3);
            let messages: Vec<String> = events
                .iter()
                .map(|e| {
                    e.as_log()[log_schema().message_key().unwrap().to_string()]
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            assert!(messages.contains(&"line1".to_string()));
            assert!(messages.contains(&"line2".to_string()));
            assert!(messages.contains(&"line3".to_string()));

            // All events share the channel from the request header.
            for event in &events {
                assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
            }
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_event_endpoint_envelope_without_time_has_fallback_timestamp() {
        // Regression: with a decoder set, an envelope that omits `time` must still
        // produce events with a timestamp (the legacy /event path always wrote one).
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                EndpointCodecConfig::default(),
            )
            .await;
            let envelope = r#"{"event":"{\"foo\":\"bar\"}"}"#;
            assert_eq!(
                200,
                post(address, "services/collector/event", envelope).await
            );

            let event = collect_n(source, 1).await.remove(0);
            assert!(
                event.as_log().get_timestamp().is_some(),
                "decoded event from envelope without `time` field is missing a timestamp"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_independent_per_endpoint_codecs() {
        // /event and /raw can be configured with completely different codecs and
        // each endpoint applies only its own. Here /event uses JSON decoding (so a
        // string `event` field decodes to fields) and /raw uses newline framing
        // with a bytes decoder (so a multi-line body fans out to N events).
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                codec_full(
                    Some(FramingConfig::NewlineDelimited(Default::default())),
                    Some(DeserializerConfig::Bytes),
                ),
            )
            .await;

            // /event: JSON decoder turns the inner string into structured fields.
            assert_eq!(
                200,
                post(
                    address,
                    "services/collector/event",
                    r#"{"event":"{\"foo\":\"bar\"}"}"#
                )
                .await
            );
            // /raw: newline framing splits the body into three events.
            assert_eq!(
                200,
                post(address, "services/collector/raw", "a\nb\nc").await
            );

            let events = collect_n(source, 4).await;
            assert_eq!(events.len(), 4);

            // The /event request produces one log with `foo=bar`.
            let event_log = events
                .iter()
                .find(|e| e.as_log().contains("foo"))
                .expect("expected /event request to produce a log with `foo` set");
            assert_eq!(event_log.as_log()["foo"], "bar".into());

            // The /raw request produces three logs whose messages are the lines.
            let raw_messages: Vec<String> = events
                .iter()
                .filter(|e| !e.as_log().contains("foo"))
                .map(|e| {
                    e.as_log()[log_schema().message_key().unwrap().to_string()]
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            assert_eq!(raw_messages.len(), 3);
            assert!(raw_messages.contains(&"a".to_string()));
            assert!(raw_messages.contains(&"b".to_string()));
            assert!(raw_messages.contains(&"c".to_string()));
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_raw_endpoint_event_has_fallback_timestamp() {
        // Regression: decoded /raw events must carry an ingest timestamp like the
        // legacy raw_event path did via `insert_standard_vector_source_metadata`.
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (source, address, _guard) = source_with_codec(
                EndpointCodecConfig::default(),
                codec_full(None, Some(DeserializerConfig::Bytes)),
            )
            .await;
            let body = "hello";
            assert_eq!(200, post(address, "services/collector/raw", body).await);

            let event = collect_n(source, 1).await.remove(0);
            assert!(
                event.as_log().get_timestamp().is_some(),
                "decoded /raw event is missing a timestamp"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn decoder_raw_endpoint_empty_decode_does_not_ack() {
        // Regression: when the decoder produces zero events from a raw payload and
        // acknowledgements are enabled, the response must not include an `ackId`
        // because /services/collector/ack would otherwise report success for data
        // Vector silently dropped.
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (sender, _recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let (_guard, address) = next_addr();
        let cx = SourceContext::new_test(sender, None);
        tokio::spawn(async move {
            SplunkConfig {
                address,
                token: Some(TOKEN.to_owned().into()),
                valid_tokens: None,
                tls: None,
                acknowledgements: ack_config,
                store_hec_token: false,
                log_namespace: None,
                keepalive: Default::default(),
                event: EndpointCodecConfig::default(),
                raw: codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            }
            .build(cx)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        // A body the JSON decoder cannot parse - codec drops it, no events emitted.
        let body = "not json {";
        let response = send_with_response(address, "services/collector/raw", body, TOKEN, &opts)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(response["code"].as_u64(), Some(0), "response: {response:?}");
        assert!(
            response.get("ackId").is_none(),
            "expected no ackId in response when decoder produced zero events, got: {response:?}"
        );
    }

    #[tokio::test]
    async fn decoder_raw_endpoint_partial_decode_does_not_ack() {
        // Regression: a request whose body decodes into some valid frames AND some
        // dropped frames (e.g., `valid \n invalid \n valid` under newline framing
        // with a JSON decoder) must not return an `ackId`. Otherwise
        // /services/collector/ack reports success for data Vector silently dropped.
        let ack_config = HecAcknowledgementsConfig {
            enabled: Some(true),
            ..Default::default()
        };
        let (sender, _recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let (_guard, address) = next_addr();
        let cx = SourceContext::new_test(sender, None);
        tokio::spawn(async move {
            SplunkConfig {
                address,
                token: Some(TOKEN.to_owned().into()),
                valid_tokens: None,
                tls: None,
                acknowledgements: ack_config,
                store_hec_token: false,
                log_namespace: None,
                keepalive: Default::default(),
                event: EndpointCodecConfig::default(),
                raw: codec_full(
                    Some(FramingConfig::NewlineDelimited(Default::default())),
                    Some(vector_lib::codecs::JsonDeserializerConfig::default().into()),
                ),
            }
            .build(cx)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        // Two valid JSON frames bracketing one invalid frame.
        let body = "{\"valid\":1}\nnot json\n{\"valid\":2}";
        let response = send_with_response(address, "services/collector/raw", body, TOKEN, &opts)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(response["code"].as_u64(), Some(0), "response: {response:?}");
        assert!(
            response.get("ackId").is_none(),
            "expected no ackId when the decoder dropped a frame mid-request, got: {response:?}"
        );
    }

    #[tokio::test]
    async fn decoder_event_endpoint_error_index_matches_envelope_not_fanout() {
        // Regression: with the decoder fanning out one envelope into many events,
        // `InvalidEventNumber` in error responses must still report the failing
        // envelope's zero-indexed position, not the cumulative event count.
        let (source, address, _guard) = source_with_codec(
            codec_full(
                Some(FramingConfig::NewlineDelimited(Default::default())),
                Some(DeserializerConfig::Bytes),
            ),
            EndpointCodecConfig::default(),
        )
        .await;
        // Envelope 0 has an `event` string with three lines: with newline framing
        // and a bytes decoder, that fans out to three events. Envelope 1 omits
        // `event`, so the decoded path returns `MissingEventField { event: 1 }`.
        let body = "{\"event\":\"a\\nb\\nc\"}{\"foo\":\"bar\"}";

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let response =
            send_with_response(address, "services/collector/event", body, TOKEN, &opts).await;
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap();

        assert_eq!(status.as_u16(), 400, "body: {body:?}");
        assert_eq!(
            body["invalid-event-number"].as_u64(),
            Some(1),
            "expected envelope index 1 (the failing envelope), not a fan-out event index. body: {body:?}"
        );
        // Drain the partially-emitted events so the source task doesn't block.
        let _ = collect_n(source, 3).await;
    }

    #[test]
    fn output_schema_definition_with_decoder_vector_namespace() {
        let config = SplunkConfig {
            log_namespace: Some(true),
            event: codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            ..Default::default()
        };
        let definition = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        // The decoder's schema produces `Kind::json()` at the root, the source
        // layers its envelope metadata fields on top, and the legacy log shape is
        // unioned in (since /raw has no decoder and still emits legacy events) -
        // contributing the `message` meaning at root.
        let expected_definition =
            Definition::new_with_default_metadata(Kind::json(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), meaning::MESSAGE)
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("splunk_hec", "host"),
                    Kind::bytes(),
                    Some("host"),
                )
                .with_metadata_field(
                    &owned_value_path!("splunk_hec", "index"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("splunk_hec", "source"),
                    Kind::bytes(),
                    Some("service"),
                )
                .with_metadata_field(
                    &owned_value_path!("splunk_hec", "channel"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("splunk_hec", "sourcetype"),
                    Kind::bytes(),
                    None,
                );

        assert_eq!(definition, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = SplunkConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definition = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()).or_bytes(),
            [LogNamespace::Vector],
        )
        .with_meaning(OwnedTargetPath::event_root(), meaning::MESSAGE)
        .with_metadata_field(
            &owned_value_path!("vector", "source_type"),
            Kind::bytes(),
            None,
        )
        .with_metadata_field(
            &owned_value_path!("vector", "ingest_timestamp"),
            Kind::timestamp(),
            None,
        )
        .with_metadata_field(
            &owned_value_path!("splunk_hec", "host"),
            Kind::bytes(),
            Some("host"),
        )
        .with_metadata_field(
            &owned_value_path!("splunk_hec", "index"),
            Kind::bytes(),
            None,
        )
        .with_metadata_field(
            &owned_value_path!("splunk_hec", "source"),
            Kind::bytes(),
            Some("service"),
        )
        .with_metadata_field(
            &owned_value_path!("splunk_hec", "channel"),
            Kind::bytes(),
            None,
        )
        .with_metadata_field(
            &owned_value_path!("splunk_hec", "sourcetype"),
            Kind::bytes(),
            None,
        );

        assert_eq!(definition, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = SplunkConfig::default();
        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(&owned_value_path!("host"), Kind::bytes(), Some("host"))
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes().or_undefined(),
            Some("message"),
        )
        .with_event_field(
            &owned_value_path!("line"),
            Kind::array(Collection::empty())
                .or_object(Collection::empty())
                .or_undefined(),
            None,
        )
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("splunk_channel"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("splunk_index"), Kind::bytes(), None)
        .with_event_field(
            &owned_value_path!("splunk_source"),
            Kind::bytes(),
            Some("service"),
        )
        .with_event_field(&owned_value_path!("splunk_sourcetype"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None);

        assert_eq!(definitions, Some(expected_definition));
    }

    impl ValidatableComponent for SplunkConfig {
        fn validation_configuration() -> ValidationConfiguration {
            let config = Self {
                address: default_socket_address(),
                ..Default::default()
            };

            let listen_addr_http = format!("http://{}/services/collector/event", config.address);
            let uri = Uri::try_from(&listen_addr_http).expect("should not fail to parse URI");

            let log_namespace: LogNamespace = config.log_namespace.unwrap_or_default().into();
            let framing = BytesDecoderConfig::new().into();
            let decoding = DeserializerConfig::Json(Default::default());

            let external_resource = ExternalResource::new(
                ResourceDirection::Push,
                HttpResourceConfig::from_parts(uri, None).with_headers(HashMap::from([(
                    X_SPLUNK_REQUEST_CHANNEL.to_string(),
                    "channel".to_string(),
                )])),
                DecodingConfig::new(framing, decoding, false.into()),
            );

            ValidationConfiguration::from_source(
                Self::NAME,
                log_namespace,
                vec![ComponentTestCaseConfig::from_source(
                    config,
                    None,
                    Some(external_resource),
                )],
            )
        }
    }

    register_validatable_component!(SplunkConfig);
}
