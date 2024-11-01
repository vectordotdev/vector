use std::time::Duration;

use tokio::{select, sync::mpsc, task::JoinHandle};
use vector_lib::event::Event;

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

const SHUTDOWN_TICKS: u8 = 3;

/// Telemetry collector for a component under validation.
pub struct Telemetry {
    listen_addr: GrpcAddress,
    service: VectorServer<EventForwardService>,
    rx: mpsc::Receiver<Vec<Event>>,
}

impl Telemetry {
    /// Creates a telemetry collector by attaching the relevant components to an existing `ConfigBuilder`.
    pub fn attach_to_config(config_builder: &mut ConfigBuilder) -> Self {
        let listen_addr = GrpcAddress::from(next_addr());
        info!(%listen_addr, "Attaching telemetry components.");

        // Attach an internal logs and internal metrics source, and send them on to a dedicated Vector
        // sink that we'll spawn a listener for to collect everything.
        let internal_logs = InternalLogsConfig::default();
        let internal_metrics = InternalMetricsConfig {
            scrape_interval_secs: Duration::from_millis(100),
            ..Default::default()
        };
        let mut vector_sink = VectorSinkConfig::from_address(listen_addr.as_uri());

        // We want to ensure that the output sink is flushed as soon as
        // possible, so we set the batch timeout to a very low value. We also
        // disable retries, as we don't want to waste time performing retries,
        // especially when the test harness is shutting down.
        vector_sink.batch.timeout_secs = Some(0.1);
        vector_sink.request.retry_attempts = 0;

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

    pub async fn into_collector(
        self,
        telemetry_task_coordinator: &TaskCoordinator<Configuring>,
    ) -> TelemetryCollector {
        let telemetry_started = telemetry_task_coordinator.track_started();
        let telemetry_completed = telemetry_task_coordinator.track_completed();
        let mut telemetry_shutdown_handle = telemetry_task_coordinator.register_for_shutdown();

        // We need a task coordinator for the gRPC server because it strictly
        // needs to be shut down after the telemetry collector. This is because
        // the server needs to be alive to process every last incoming event
        // from the Vector sink that we're using to collect telemetry.
        let grpc_task_coordinator = TaskCoordinator::new("gRPC");
        spawn_grpc_server(self.listen_addr, self.service, &grpc_task_coordinator);
        let mut grpc_task_coordinator = grpc_task_coordinator.started().await;
        info!("All gRPC task(s) started.");

        let mut rx = self.rx;
        let driver_handle = tokio::spawn(async move {
            telemetry_started.mark_as_done();

            let mut telemetry_events = Vec::new();
            'outer: loop {
                select! {
                    _ = telemetry_shutdown_handle.wait() => {
                        // After we receive the shutdown signal, we need to wait
                        // for two batches of event emissions from the internal_metrics
                        // source. This is to ensure that we've received all the
                        // events from the components that we're testing.
                        //
                        // We need exactly two because the internal_metrics
                        // source does not emit component events until after the
                        // component_received_events_total metric has been
                        // emitted. Thus, two batches ensure that all component
                        // events have been emitted.

                        info!("Telemetry: waiting for final internal_metrics events before shutting down.");

                        let mut batches_received = 0;

                        let timeout = tokio::time::sleep(Duration::from_secs(5));
                        tokio::pin!(timeout);

                        loop {
                            select! {
                                d = rx.recv() => {
                                    match d {
                                        None => break,
                                        Some(telemetry_event_batch) => {
                                        telemetry_events.extend(telemetry_event_batch);
                                            info!("Telemetry: processed one batch of internal_metrics.");
                                            batches_received += 1;
                                            if batches_received == SHUTDOWN_TICKS {
                                                break;
                                            }
                                        }
                                    }
                                },
                                _ = &mut timeout => break,
                            }
                        }
                        if batches_received != SHUTDOWN_TICKS {
                            panic!("Did not receive {SHUTDOWN_TICKS} events while waiting for shutdown! Only received {batches_received}!");
                        }
                        break 'outer;
                    },
                    maybe_telemetry_event = rx.recv() => match maybe_telemetry_event {
                        None => break,
                        Some(telemetry_event_batch) => telemetry_events.extend(telemetry_event_batch),
                    },
                }
            }

            grpc_task_coordinator.shutdown().await;

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
