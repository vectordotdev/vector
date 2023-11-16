use std::{borrow::Cow, collections::BTreeMap, time::Duration};

use colored::{ColoredString, Colorize};
use tokio_stream::StreamExt;
use url::Url;
use vector_lib::api_client::{
    connect_subscription_client,
    gql::{
        output_events_by_component_id_patterns_subscription::OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns,
        TapEncodingFormat, TapSubscriptionExt,
    },
    Client,
};

use crate::signal::{SignalRx, SignalTo};

/// Delay (in milliseconds) before attempting to reconnect to the Vector API
const RECONNECT_DELAY: u64 = 5000;

/// CLI command func for issuing 'tap' queries, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets.
pub(crate) async fn cmd(opts: &super::Opts, signal_rx: SignalRx) -> exitcode::ExitCode {
    let url = opts.url();
    // Return early with instructions for enabling the API if the endpoint isn't reachable
    // via a healthcheck.
    let client = Client::new(url.clone());
    #[allow(clippy::print_stderr)]
    if client.healthcheck().await.is_err() {
        eprintln!(
            indoc::indoc! {"
            Vector API server isn't reachable ({}).

            Have you enabled the API?

            To enable the API, add the following to your Vector config file:

            [api]
                enabled = true"},
            url
        );
        return exitcode::UNAVAILABLE;
    }

    tap(opts, signal_rx).await
}

/// Observe event flow from specified components
pub async fn tap(opts: &super::Opts, mut signal_rx: SignalRx) -> exitcode::ExitCode {
    let subscription_url = opts.web_socket_url();
    let formatter = EventFormatter::new(opts.meta, opts.format);
    let outputs_patterns = opts.outputs_patterns();

    loop {
        tokio::select! {
            biased;
            Ok(SignalTo::Shutdown(_) | SignalTo::Quit) = signal_rx.recv() => break,
            status = run(subscription_url.clone(), opts, outputs_patterns.clone(), formatter.clone()) => {
                if status == exitcode::UNAVAILABLE || status == exitcode::TEMPFAIL && !opts.no_reconnect {
                    #[allow(clippy::print_stderr)]
                    {
                        eprintln!("[tap] Connection failed. Reconnecting in {:?} seconds.", RECONNECT_DELAY / 1000);
                    }
                    tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY)).await;
                } else {
                    break;
                }
            }
        }
    }

    exitcode::OK
}

async fn run(
    url: Url,
    opts: &super::Opts,
    outputs_patterns: Vec<String>,
    formatter: EventFormatter,
) -> exitcode::ExitCode {
    let subscription_client = match connect_subscription_client(url).await {
        Ok(c) => c,
        Err(e) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("[tap] Couldn't connect to API via WebSockets: {}", e);
            }
            return exitcode::UNAVAILABLE;
        }
    };

    tokio::pin! {
        let stream = subscription_client.output_events_by_component_id_patterns_subscription(
            outputs_patterns,
            opts.inputs_of.clone(),
            opts.format,
            opts.limit as i64,
            opts.interval as i64,
        );
    };

    // Loop over the returned results, printing out tap events.
    #[allow(clippy::print_stdout)]
    #[allow(clippy::print_stderr)]
    loop {
        let message = stream.next().await;
        if let Some(Some(res)) = message {
            if let Some(d) = res.data {
                for tap_event in d.output_events_by_component_id_patterns.iter() {
                    match tap_event {
                        OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::Log(ev) => {
                            println!("{}", formatter.format(ev.component_id.as_ref(), ev.component_kind.as_ref(), ev.component_type.as_ref(), ev.string.as_ref()));
                        },
                        OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::Metric(ev) => {
                            println!("{}", formatter.format(ev.component_id.as_ref(), ev.component_kind.as_ref(), ev.component_type.as_ref(), ev.string.as_ref()));
                        },
                        OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::Trace(ev) => {
                            println!("{}", formatter.format(ev.component_id.as_ref(), ev.component_kind.as_ref(), ev.component_type.as_ref(), ev.string.as_ref()));
                        },
                        OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::EventNotification(ev) => {
                            if !opts.quiet {
                                eprintln!("{}", ev.message);
                            }
                        },
                    }
                }
            }
        } else {
            return exitcode::TEMPFAIL;
        }
    }
}

#[derive(Clone)]
struct EventFormatter {
    meta: bool,
    format: TapEncodingFormat,
    component_id_label: ColoredString,
    component_kind_label: ColoredString,
    component_type_label: ColoredString,
}

impl EventFormatter {
    fn new(meta: bool, format: TapEncodingFormat) -> Self {
        Self {
            meta,
            format,
            component_id_label: "component_id".green(),
            component_kind_label: "component_kind".green(),
            component_type_label: "component_type".green(),
        }
    }

    fn format<'a>(
        &self,
        component_id: &str,
        component_kind: &str,
        component_type: &str,
        event: &'a str,
    ) -> Cow<'a, str> {
        if self.meta {
            match self.format {
                TapEncodingFormat::Json => format!(
                    r#"{{"{}":"{}","{}":"{}","{}":"{}","event":{}}}"#,
                    self.component_id_label,
                    component_id.green(),
                    self.component_kind_label,
                    component_kind.green(),
                    self.component_type_label,
                    component_type.green(),
                    event
                )
                .into(),
                TapEncodingFormat::Yaml => {
                    let mut value: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
                    value.insert("event".to_string(), serde_yaml::from_str(event).unwrap());
                    // We interpolate to include component_id rather than
                    // include it in the map to correctly preserve color
                    // formatting
                    format!(
                        "{}{}: {}\n{}: {}\n{}: {}\n",
                        serde_yaml::to_string(&value).unwrap(),
                        self.component_id_label,
                        component_id.green(),
                        self.component_kind_label,
                        component_kind.green(),
                        self.component_type_label,
                        component_type.green()
                    )
                    .into()
                }
                TapEncodingFormat::Logfmt => format!(
                    "{}={} {}={} {}={} {}",
                    self.component_id_label,
                    component_id.green(),
                    self.component_kind_label,
                    component_kind.green(),
                    self.component_type_label,
                    component_type.green(),
                    event
                )
                .into(),
            }
        } else {
            event.into()
        }
    }
}
