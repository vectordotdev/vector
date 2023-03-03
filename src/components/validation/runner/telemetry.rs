use tokio::{select, sync::mpsc, task::JoinHandle};
use vector_core::event::Event;

use crate::{
    components::validation::{
        sync::{Configuring, TaskCoordinator},
        util::GrpcAddress,
    },
    proto::vector::Server as VectorServer,
};
use crate::{
    config::ConfigBuilder,
    sinks::vector::VectorConfig as VectorSinkConfig,
    sources::{internal_logs::InternalLogsConfig, internal_metrics::InternalMetricsConfig},
    test_util::next_addr,
};

use super::io::{spawn_grpc_server, EventForwardService};

const INTERNAL_LOGS_KEY: &str = "_telemetry_logs";
const INTERNAL_METRICS_KEY: &str = "_telemetry_metrics";
const VECTOR_SINK_KEY: &str = "_telemetry_out";

/// Telemetry collector for a component under validation.
pub struct Telemetry {
    listen_addr: GrpcAddress,
    service: VectorServer<EventForwardService>,
    rx: mpsc::Receiver<Event>,
}

impl Telemetry {
    /// Creates a telemetry collector by attaching the relevant components to an existing `ConfigBuilder`.
    pub fn attach_to_config(config_builder: &mut ConfigBuilder) -> Self {
        let listen_addr = GrpcAddress::from(next_addr());
        debug!(%listen_addr, "Attaching telemetry components.");

        // Attach an internal logs and internal metrics source, and send them on to a dedicated Vector
        // sink that we'll spawn a listener for to collect everything.
        let internal_logs = InternalLogsConfig::default();
        let internal_metrics = InternalMetricsConfig::default();
        let vector_sink = VectorSinkConfig::from_address(listen_addr.as_uri());

        config_builder.add_source(INTERNAL_LOGS_KEY, internal_logs);
        config_builder.add_source(INTERNAL_METRICS_KEY, internal_metrics);
        config_builder.add_sink(
            VECTOR_SINK_KEY,
            &[INTERNAL_LOGS_KEY, INTERNAL_METRICS_KEY],
            vector_sink,
        );

        let (tx, rx) = mpsc::channel(1024);

        Self {
            listen_addr,
            service: VectorServer::new(EventForwardService::from(tx)),
            rx,
        }
    }

    pub fn into_collector(
        self,
        task_coordinator: &TaskCoordinator<Configuring>,
    ) -> TelemetryCollector {
        let telemetry_started = task_coordinator.track_started();
        let telemetry_completed = task_coordinator.track_completed();
        let mut telemetry_shutdown_handle = task_coordinator.register_for_shutdown();

        spawn_grpc_server(self.listen_addr, self.service, task_coordinator);

        let mut rx = self.rx;
        let driver_handle = tokio::spawn(async move {
            telemetry_started.mark_as_done();

            let mut telemetry_events = Vec::new();
            loop {
                select! {
                    _ = telemetry_shutdown_handle.wait() => break,
                    maybe_telemetry_event = rx.recv() => match maybe_telemetry_event {
                        None => break,
                        Some(telemetry_event) => telemetry_events.push(telemetry_event),
                    },
                }
            }

            telemetry_completed.mark_as_done();

            telemetry_events
        });

        TelemetryCollector { driver_handle }
    }
}

pub struct TelemetryCollector {
    driver_handle: JoinHandle<Vec<Event>>,
}

impl TelemetryCollector {
    pub async fn collect(self) -> Vec<Event> {
        self.driver_handle
            .await
            .expect("telemetry collector task should not panic")
    }
}
