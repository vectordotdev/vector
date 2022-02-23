use std::collections::BTreeMap;

use colored::{ColoredString, Colorize};
use tokio_stream::StreamExt;
use url::Url;
use vector_api_client::{
    connect_subscription_client,
    gql::{
        output_events_by_component_id_patterns_subscription::{
            EventNotificationType,
            OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns,
        },
        TapEncodingFormat, TapSubscriptionExt,
    },
    Client,
};

use crate::{
    config,
    signal::{SignalRx, SignalTo},
};

/// CLI command func for issuing 'tap' queries, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets.
pub async fn cmd(opts: &super::Opts, mut signal_rx: SignalRx) -> exitcode::ExitCode {
    // Use the provided URL as the Vector GraphQL API server, or default to the local port
    // provided by the API config. This will work despite `api` and `api-client` being distinct
    // features; the config is available even if `api` is disabled.
    let mut url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_address().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr))
            .expect("Couldn't parse default API URL. Please report this.")
    });

    // Return early with instructions for enabling the API if the endpoint isn't reachable
    // via a healthcheck.
    if Client::new_with_healthcheck(url.clone()).await.is_none() {
        return exitcode::UNAVAILABLE;
    }

    // Change the HTTP schema to WebSockets.
    url.set_scheme(match url.scheme() {
        "https" => "wss",
        _ => "ws",
    })
    .expect("Couldn't build WebSocket URL. Please report.");

    let subscription_client = match connect_subscription_client(url).await {
        Ok(c) => c,
        Err(e) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Couldn't connect to Vector API via WebSockets: {:?}", e);
            }
            return exitcode::UNAVAILABLE;
        }
    };

    // If no patterns are provided, tap all components' outputs
    let outputs_patterns = if opts.component_id_patterns.is_empty()
        && opts.outputs_of.is_empty()
        && opts.inputs_of.is_empty()
    {
        vec!["*".to_string()]
    } else {
        opts.outputs_of
            .iter()
            .cloned()
            .chain(opts.component_id_patterns.iter().cloned())
            .collect()
    };

    // Issue the 'tap' request, printing to stdout.
    let res = subscription_client.output_events_by_component_id_patterns_subscription(
        outputs_patterns,
        opts.inputs_of.clone(),
        opts.format.clone(),
        opts.limit as i64,
        opts.interval as i64,
    );

    tokio::pin! {
        let stream = res.stream();
    };

    let formatter = EventFormatter::new(opts.format.clone());

    // Loop over the returned results, printing out tap events.
    loop {
        tokio::select! {
            biased;
            Some(SignalTo::Shutdown | SignalTo::Quit) = signal_rx.recv() => break,
            Some(Some(res)) = stream.next() => {
                if let Some(d) = res.data {
                    for tap_event in d.output_events_by_component_id_patterns.iter() {
                        match tap_event {
                            OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::Log(ev) => {
                                println!("{}", formatter.format(ev.component_id.as_ref(), ev.string.as_ref()))
                            },
                            OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::Metric(ev) => {
                                println!("{}", formatter.format(ev.component_id.as_ref(), ev.string.as_ref()))
                            },
                            OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::Trace(ev) => {
                                println!("{}", formatter.format(ev.component_id.as_ref(), ev.string.as_ref()))
                            },
                            OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns::EventNotification(ev) => {
                                if !opts.quiet {
                                    match ev.notification {
                                        EventNotificationType::MATCHED => eprintln!(r#"[tap] Pattern "{}" successfully matched."#, ev.pattern),
                                        EventNotificationType::NOT_MATCHED => eprintln!(r#"[tap] Pattern "{}" failed to match: will retry on configuration reload."#, ev.pattern),
                                        EventNotificationType::Other(_) => {},
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }

    exitcode::OK
}

struct EventFormatter {
    format: TapEncodingFormat,
    component_id_label: ColoredString,
}

impl EventFormatter {
    fn new(format: TapEncodingFormat) -> Self {
        Self {
            format,
            component_id_label: "component_id".green(),
        }
    }

    fn format(&self, component_id: &str, event: &str) -> String {
        match self.format {
            TapEncodingFormat::Json => format!(
                r#"{{"{}":"{}","event":{}}}"#,
                self.component_id_label,
                component_id.green(),
                event
            ),
            TapEncodingFormat::Yaml => {
                let mut value: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
                value.insert("event".to_string(), serde_yaml::from_str(event).unwrap());
                format!(
                    "{}{}: {}\n",
                    serde_yaml::to_string(&value).unwrap(),
                    self.component_id_label,
                    component_id.green()
                )
            }
            TapEncodingFormat::Logfmt => format!(
                "{}={} {}",
                self.component_id_label,
                component_id.green(),
                event
            ),
        }
    }
}
