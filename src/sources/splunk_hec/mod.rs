use std::{
    collections::HashMap,
    convert::Infallible,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use chrono::{DateTime, TimeZone, Utc};
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
    event::{BatchNotifier, BatchStatusReceiver, EventMetadata},
    internal_event::{
        ComponentEventsDropped, CountByteSize, InternalEventHandle as _, Registered, UNINTENTIONAL,
    },
    lookup::{
        self, OwnedValuePath, event_path, lookup_v2::OptionalValuePath, metadata_path,
        owned_value_path,
    },
    schema::meaning,
    sensitive_string::SensitiveString,
    source_sender::SendError,
    tls::MaybeTlsIncomingStream,
};
use vrl::{
    path::{OwnedTargetPath, PathPrefix, ValuePath as _},
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
    common::http::ErrorMessage,
    config::{DataType, Resource, SourceConfig, SourceContext, SourceOutput, log_schema},
    event::{Event, LogEvent, Value},
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer},
    internal_events::{
        EventsReceived, HttpBytesReceived, SplunkHecRequestBodyInvalidError, SplunkHecRequestError,
    },
    serde::bool_or_struct,
    sources::util::{decompression::CappedDecoder, http::capped_body},
    tls::{MaybeTlsSettings, TlsAcceptorReloader, TlsEnableableConfig},
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
    /// When `decoding` is set, Vector applies a second decoding pass after parsing the
    /// HEC envelope. The envelope's `event` field is passed through the codec,
    /// and a single envelope can fan out to multiple events. Decode failures are
    /// swallowed and do not return an error to the Splunk client.
    ///
    /// The VRL codec can access HEC envelope metadata, such as host, sourcetype, and,
    /// channel, and the authentication token via `%splunk_hec.*` paths and
    /// `get_secret!("splunk_hec_token")` before the program executes.
    #[configurable(derived)]
    #[serde(default)]
    pub event: CodecConfig,

    /// Codec configuration applied to events received on `/services/collector/raw`.
    ///
    /// When `decoding` is set, the (decompressed) request body is fed through the
    /// codec instead of being emitted as a single event. Decode failures are
    /// swallowed and do not return an error to the Splunk client. When unset, the
    /// endpoint preserves its existing behavior of one event per request body.
    #[configurable(derived)]
    #[serde(default)]
    pub raw: CodecConfig,
}

/// Codec configuration applied to one of the `splunk_hec` endpoints.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct CodecConfig {
    /// Framing configuration applied to the payload.
    ///
    /// Only used when `decoding` is also set. Defaults to a per-codec choice
    /// (typically `bytes`) that produces one event per payload.
    #[configurable(derived)]
    #[serde(default)]
    pub framing: Option<FramingConfig>,

    /// Decoding configuration applied to the payload.
    ///
    /// When unset, the endpoint preserves its existing per-endpoint default
    /// behavior. When set, the endpoint-selected payload is processed through
    /// `framing` and `decoding`, and a single payload can fan out to multiple
    /// events.
    #[configurable(derived)]
    #[serde(default)]
    pub decoding: Option<DeserializerConfig>,
}

impl CodecConfig {
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
            event: CodecConfig::default(),
            raw: CodecConfig::default(),
        }
    }
}

fn default_socket_address() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 8088)
}

impl SplunkConfig {
    /// The source's TLS configuration, if any. Exposed so a wrapping source can build a
    /// [`TlsAcceptorReloader`] and watch the certificate files for rotation.
    pub const fn tls_config(&self) -> Option<&TlsEnableableConfig> {
        self.tls.as_ref()
    }

    /// Build the source serving a runtime-swappable TLS acceptor when `tls_reloader` is set.
    pub async fn build_with_tls_reloader(
        &self,
        cx: SourceContext,
        tls_reloader: Option<TlsAcceptorReloader>,
    ) -> crate::Result<super::Source> {
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

        let listener = tls.bind_reloadable(&self.address, tls_reloader).await?;

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
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec")]
impl SourceConfig for SplunkConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        self.build_with_tls_reloader(cx, None).await
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        // Build schemas per endpoint, then merge them. Each endpoint decides at
        // runtime whether source metadata overwrites event fields or defers to a
        // decoder-produced value, so applying one global strategy would make mixed
        // decoder/no-decoder configurations advertise the wrong contract.
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

        let endpoint_base = |decoding: &Option<DeserializerConfig>| match decoding {
            Some(decoding) => decoding.schema_definition(log_namespace),
            None => legacy_base(),
        };

        let splunk_legacy_key = |path: OwnedValuePath, has_decoder: bool| {
            if has_decoder {
                LegacyKey::InsertIfEmpty(path)
            } else {
                LegacyKey::Overwrite(path)
            }
        };

        let add_common_metadata = |definition: vector_lib::schema::Definition| {
            definition
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
        };

        let add_channel_metadata = |definition: vector_lib::schema::Definition,
                                    has_decoder: bool| {
            definition.with_source_metadata(
                SplunkConfig::NAME,
                Some(splunk_legacy_key(owned_value_path!(CHANNEL), has_decoder)),
                &owned_value_path!("channel"),
                Kind::bytes(),
                None,
            )
        };

        let event_has_decoder = self.event.decoding.is_some();
        let raw_has_decoder = self.raw.decoding.is_some();

        // Merge the per-endpoint base schemas (event root kind + standard Vector
        // metadata). Splunk-specific fields are added once afterward with
        // per-field decoder flags, avoiding the widening that occurs when the
        // raw schema's open `metadata_kind.unknown` overrides specific fields
        // from the event schema during merge.
        let merged_base = add_common_metadata(
            endpoint_base(&self.event.decoding).merge(endpoint_base(&self.raw.decoding)),
        );

        // `index`, `source`, `sourcetype` are only written by the /event endpoint.
        // `channel` is written by both; use Overwrite if either endpoint has no
        // decoder (some events still overwrite it).
        let channel_has_decoder = event_has_decoder && raw_has_decoder;
        let schema_definition = add_channel_metadata(
            merged_base
                .with_source_metadata(
                    SplunkConfig::NAME,
                    Some(splunk_legacy_key(
                        owned_value_path!(INDEX),
                        event_has_decoder,
                    )),
                    &owned_value_path!("index"),
                    Kind::bytes(),
                    None,
                )
                .with_source_metadata(
                    SplunkConfig::NAME,
                    Some(splunk_legacy_key(
                        owned_value_path!(SOURCE),
                        event_has_decoder,
                    )),
                    &owned_value_path!("source"),
                    Kind::bytes(),
                    Some(meaning::SERVICE),
                )
                // Not to be confused with `source_type`.
                .with_source_metadata(
                    SplunkConfig::NAME,
                    Some(splunk_legacy_key(
                        owned_value_path!(SOURCETYPE),
                        event_has_decoder,
                    )),
                    &owned_value_path!("sourcetype"),
                    Kind::bytes(),
                    None,
                ),
            channel_has_decoder,
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
            .and(capped_body())
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

                        let data;
                        let (byte_size, body) = if gzip {
                            // Cap the decompressed output to mitigate gzip-bomb DoS.
                            data = CappedDecoder::gzip(body.reader())
                                .decompress()
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
                            maybe_ack_id =
                                register_ack(idx_ack.clone(), receiver.take(), channel.clone())
                                    .await?;
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
                        // frames. Also skip ack registration when a later envelope
                        // errored even if earlier ones produced events: the client
                        // gets a 400 and never sees the ack id, so registering it
                        // only leaks pending-ack capacity.
                        if decoder_in_use {
                            maybe_ack_id =
                                if events.is_empty() || had_decode_errors || error.is_some() {
                                    drop(receiver);
                                    None
                                } else {
                                    register_ack(idx_ack, receiver, channel).await?
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
            .and(capped_body())
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
                            let maybe_ack_id =
                                register_ack(idx_ack, receiver, Some(channel_id.clone())).await?;
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

                        // Decoder path: pass the optional HEC token into raw_event so
                        // it's stamped on each event the moment it leaves the codec
                        // (rather than after the whole payload is decoded).
                        let token: Option<Arc<str>> =
                            token.filter(|_| store_hec_token).map(Arc::from);
                        let (events, had_decode_errors) = raw_event(
                            body,
                            gzip,
                            channel_id.clone(),
                            remote,
                            xff,
                            batch,
                            log_namespace,
                            &events_received,
                            Some(decoder),
                            token,
                        )?;

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

                        let maybe_ack_id =
                            register_ack(idx_ack, receiver, Some(channel_id)).await?;

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
            .and(capped_body())
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
    /// Optional second-stage decoder applied to the envelope payload after HEC
    /// envelope parsing.
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

    /// Build an `EventMetadata` template from the current envelope context so
    /// that VRL decoders can read source-supplied values via `%`-prefixed paths
    /// before the decoder program executes.
    ///
    /// Peeks at the envelope `json` without consuming any fields (consumption
    /// happens later in `build_events_decoded`). Falls back to sticky extractor
    /// state for fields not present in the current envelope.
    fn build_vrl_metadata(&self, json: &JsonValue) -> EventMetadata {
        let mut metadata = EventMetadata::default();

        // Splunk HEC token as a secret so VRL can read it via get_secret!()
        if let Some(token) = &self.token {
            metadata.set_splunk_hec_token(Arc::clone(token));
        }

        // Envelope host/source/sourcetype/index: peek current value; fall back
        // to sticky extractor state.
        let fields: &[(&str, &str)] = &[
            ("host", "splunk_hec.host"),
            ("source", "splunk_hec.source"),
            ("sourcetype", "splunk_hec.sourcetype"),
            ("index", "splunk_hec.index"),
        ];
        for (json_key, meta_path) in fields {
            let val = json
                .get(json_key)
                .and_then(|v| v.as_str())
                .map(|s| Value::from(s.to_string()))
                .or_else(|| {
                    self.extractors
                        .iter()
                        .find(|e| e.field == *json_key)
                        .and_then(|e| e.value.clone())
                });
            if let Some(v) = val {
                metadata.value_mut().insert(
                    &vrl::path::parse_value_path(meta_path)
                        .expect("hardcoded splunk_hec metadata path is a valid VRL path"),
                    v,
                );
            }
        }

        // Channel: envelope field or header default
        let channel = json
            .get("channel")
            .and_then(|v| v.as_str())
            .map(|s| Value::from(s.to_string()))
            .or_else(|| self.channel.clone());
        if let Some(ch) = channel {
            metadata.value_mut().insert(
                &vrl::path::parse_value_path("splunk_hec.channel")
                    .expect("splunk_hec.channel is a valid VRL path"),
                ch,
            );
        }

        metadata
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
        let event = self.validate_event_field(&json)?;
        // Strings are passed as raw bytes so decoders see the bare content
        // (e.g. a JSON string event containing `{"foo":"bar"}` arrives at the
        // decoder as `{"foo":"bar"}`, not `"{\"foo\":\"bar\"}"` ). All other
        // JSON values (objects, arrays, numbers, bools) are serialized to JSON.
        let payload = if let Some(s) = event.as_str() {
            s.as_bytes().to_vec()
        } else {
            match serde_json::to_vec(event) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let error: vector_lib::Error = Box::new(error);
                    emit!(
                        vector_lib::codecs::internal_events::DecoderDeserializeError {
                            error: &error
                        }
                    );
                    emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                        count: 1,
                        reason: "Failed to serialize event field to bytes.",
                    });
                    return Ok((vec![], true));
                }
            }
        };

        self.process_time(&mut json)?;

        // Always forward a fallback timestamp so events without an explicit envelope
        // `time` field still get one (matches the legacy /event behavior, which always
        // wrote a timestamp). `decode_message` uses `try_insert`, so a decoder-supplied
        // timestamp still wins on conflict.
        let fallback_time = match self.time {
            Time::Provided(t) | Time::Now(t) => t,
        };

        // Build a metadata template so VRL decoders can read envelope context
        // via `%`-prefixed paths (e.g. `%splunk_hec.host`, `%vector.secrets.*`).
        // For non-VRL decoders `with_metadata_template` is a no-op.
        let decoder = decoder.with_metadata_template(self.build_vrl_metadata(&json));

        let (decoded, had_decode_errors) = decode_payload(
            decoder,
            &payload,
            Some(fallback_time),
            true, // /event: write %splunk_hec.timestamp
            DecodePayloadContext {
                batch: &self.batch,
                log_namespace: self.log_namespace,
                events_received: &self.events_received,
                splunk_hec_token: self.token.as_ref(),
            },
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
                // for legacy event fields, and `try_insert` for the Vector metadata
                // path so a decoder-produced `%splunk_hec.channel` survives.
                if let Some(channel_val) = envelope_channel.clone().or_else(|| self.channel.clone())
                {
                    match self.log_namespace {
                        LogNamespace::Legacy => {
                            self.log_namespace.insert_source_metadata(
                                SplunkConfig::NAME,
                                log,
                                Some(LegacyKey::InsertIfEmpty(&channel_path)),
                                lookup::path!(CHANNEL),
                                channel_val,
                            );
                        }
                        LogNamespace::Vector => {
                            log.try_insert(
                                metadata_path!(SplunkConfig::NAME, CHANNEL),
                                channel_val,
                            );
                        }
                    }
                }

                // Top-level envelope fields (host/index/source/sourcetype) must be
                // applied before `fields.*` so top-level beats `fields.*` when both
                // are present — matching the non-decoder runtime precedence. Both
                // still use InsertIfEmpty so the decoder's output wins over all
                // envelope metadata. Order: decoder > top-level > fields.
                for de in self.extractors.iter_mut() {
                    de.extract(log, &mut json);
                }

                // fields: use `InsertIfEmpty` / `try_insert` to preserve decoder-wins
                // and extractor-wins semantics (fields fill only what neither the
                // decoder nor the top-level envelope keys have already set).
                if let Some(ref fields) = envelope_fields {
                    for (key, value) in fields {
                        match self.log_namespace {
                            LogNamespace::Legacy => {
                                self.log_namespace.insert_source_metadata(
                                    SplunkConfig::NAME,
                                    log,
                                    Some(LegacyKey::InsertIfEmpty(&owned_value_path!(
                                        key.as_str()
                                    ))),
                                    lookup::path!(key.as_str()),
                                    value.clone(),
                                );
                            }
                            LogNamespace::Vector => {
                                log.try_insert(
                                    metadata_path!(SplunkConfig::NAME, key.as_str()),
                                    value.clone(),
                                );
                            }
                        }
                    }
                }
            }
            // `splunk_hec_token` is set inside `decode_payload` so the metadata is
            // attached at the moment each event leaves the codec. Don't overwrite it
            // here.
            out.push(event);
        }

        Ok((out, had_decode_errors))
    }

    /// Validate the `event` field of a HEC envelope, returning a reference to the
    /// validated value or an error if it is missing, null, or (for string values)
    /// empty. Shared between the decoder path and the legacy/vector construction
    /// paths so they all enforce the same HEC protocol contract.
    fn validate_event_field<'a>(&self, json: &'a JsonValue) -> Result<&'a JsonValue, Rejection> {
        let event_idx = self.envelopes_processed.saturating_sub(1);
        match json.get("event") {
            None | Some(JsonValue::Null) => {
                Err(ApiError::MissingEventField { event: event_idx }.into())
            }
            Some(JsonValue::String(s)) if s.is_empty() => {
                Err(ApiError::EmptyEventField { event: event_idx }.into())
            }
            Some(event) => Ok(event),
        }
    }

    /// Build the log event for the vector namespace.
    /// In this namespace the log event is created entirely from the event field.
    /// No renaming of the `line` field is done.
    fn build_log_vector(&mut self, json: &mut JsonValue) -> Result<LogEvent, Rejection> {
        let event: Value = self.validate_event_field(json)?.into();
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

    /// Build the log event for the legacy namespace.
    /// If the event is a string, or the event contains a field called `line` that is a string
    /// (the docker splunk logger places the message in the event.line field) that string
    /// is placed in the message field.
    fn build_log_legacy(&mut self, json: &mut JsonValue) -> Result<LogEvent, Rejection> {
        // validate_event_field checks for missing/null/empty-string
        self.validate_event_field(json)?;
        let mut log = LogEvent::default();
        match json["event"].take() {
            JsonValue::String(string) => {
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
        }

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

struct DecodePayloadContext<'a> {
    batch: &'a Option<BatchNotifier>,
    log_namespace: LogNamespace,
    events_received: &'a Registered<EventsReceived>,
    splunk_hec_token: Option<&'a Arc<str>>,
}

/// Run a payload through the configured `framing` + `decoding` codec.
///
/// Returns the decoded events along with a flag indicating whether any decode error
/// occurred. The shared `crate::sources::util::decode_message` helper swallows
/// decode errors silently, which is fine for sources without ack semantics, but for
/// `splunk_hec` we need to know about errors so we can refuse to acknowledge a
/// request that lost data mid-stream.
///
/// On each decoded event this helper sets `source_type`, `vector.ingest_timestamp`,
/// the optional `splunk_hec.timestamp` (only when `set_source_timestamp` is `true`,
/// i.e. for the `/event` endpoint which carries an HEC envelope `time` field), and
/// the optional Splunk HEC token. Pass `set_source_timestamp = false` for `/raw`,
/// which has no envelope timestamp and should only receive `%vector.ingest_timestamp`.
fn decode_payload(
    mut decoder: Decoder,
    payload: &[u8],
    fallback_timestamp: Option<DateTime<Utc>>,
    set_source_timestamp: bool,
    ctx: DecodePayloadContext<'_>,
) -> (Vec<Event>, bool) {
    let DecodePayloadContext {
        batch,
        log_namespace,
        events_received,
        splunk_hec_token,
    } = ctx;
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
                                // Only write %splunk_hec.timestamp for the /event
                                // endpoint, which has a real HEC envelope timestamp.
                                // /raw has no envelope time and should only get the
                                // standard %vector.ingest_timestamp below.
                                if set_source_timestamp && let Some(timestamp) = fallback_timestamp
                                {
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
                    if let Some(token) = splunk_hec_token {
                        event.metadata_mut().set_splunk_hec_token(Arc::clone(token));
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
            // For Vector namespace + InsertIfEmpty (decoder mode): check the metadata
            // value tree before inserting so VRL-produced values aren't overwritten.
            // `insert_source_metadata` for Vector ns always calls `insert`, not
            // `try_insert`, so we replicate its path construction here.
            if matches!(self.log_namespace, LogNamespace::Vector)
                && matches!(self.legacy_key_strategy, LegacyKeyStrategy::InsertIfEmpty)
            {
                log.try_insert(
                    (
                        PathPrefix::Metadata,
                        lookup::path!(SplunkConfig::NAME).concat(metadata_key),
                    ),
                    index.clone(),
                );
            } else {
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
                );
            }
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
    splunk_hec_token: Option<Arc<str>>,
) -> Result<(Vec<Event>, bool), Rejection> {
    // Process gzip
    let body_bytes: Bytes = if gzip {
        // Cap the decompressed output to mitigate gzip-bomb DoS.
        match CappedDecoder::gzip(bytes.reader()).decompress() {
            Ok(data) if data.is_empty() => return Err(ApiError::NoData.into()),
            Ok(data) => Bytes::from(data),
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
        // Build a metadata template so VRL decoders can read raw-endpoint context
        // via `%`-prefixed paths (e.g. `%splunk_hec.channel`, `%splunk_hec.host`,
        // `%vector.secrets.splunk_hec_token`). No-op for non-VRL decoders.
        let decoder = {
            let mut meta = EventMetadata::default();
            if let Some(token) = splunk_hec_token.as_ref() {
                meta.set_splunk_hec_token(Arc::clone(token));
            }
            if let Some(ref h) = host {
                meta.value_mut().insert(
                    &vrl::path::parse_value_path("splunk_hec.host")
                        .expect("splunk_hec.host is a valid VRL path"),
                    h.clone(),
                );
            }
            meta.value_mut().insert(
                &vrl::path::parse_value_path("splunk_hec.channel")
                    .expect("splunk_hec.channel is a valid VRL path"),
                channel.clone(),
            );
            decoder.with_metadata_template(meta)
        };

        // Pass ingest time as the fallback timestamp so decoded events always have
        // one - matches `insert_standard_vector_source_metadata` in the legacy raw
        // path. `decode_payload` uses `try_insert`, so a decoder-supplied timestamp
        // still wins on conflict.
        decode_payload(
            decoder,
            &body_bytes,
            Some(Utc::now()),
            false, // /raw: no HEC envelope timestamp; only %vector.ingest_timestamp
            DecodePayloadContext {
                batch: &batch,
                log_namespace,
                events_received,
                splunk_hec_token: splunk_hec_token.as_ref(),
            },
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
            // When a decoder is in use, preserve decoder-wins semantics for Vector ns
            // by using `try_insert` on the metadata path (insert_source_metadata for
            // Vector ns always overwrites). Without a decoder the log is freshly
            // constructed so overwriting is correct.
            if decoder_in_use && matches!(log_namespace, LogNamespace::Vector) {
                log.try_insert(metadata_path!(SplunkConfig::NAME, CHANNEL), channel.clone());
                if let Some(ref h) = host {
                    log.try_insert(metadata_path!(SplunkConfig::NAME, "host"), h.clone());
                }
            } else {
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

async fn register_ack(
    idx_ack: Option<Arc<IndexerAcknowledgement>>,
    receiver: Option<BatchStatusReceiver>,
    channel: Option<String>,
) -> Result<Option<u64>, Rejection> {
    match (idx_ack, receiver, channel) {
        (Some(ack), Some(rx), Some(ch)) => Ok(Some(ack.get_ack_id_from_channel(ch, rx).await?)),
        _ => Ok(None),
    }
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
    } else if let Some(error) = rejection.find::<ErrorMessage>() {
        Ok((response_json(error.status_code(), error),))
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
mod tests;
