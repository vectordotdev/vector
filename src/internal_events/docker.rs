use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct DockerEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for DockerEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received one event.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
        counter!("bytes_processed", self.byte_size as u64,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
    }
}

#[derive(Debug)]
pub struct DockerContainerEventReceived<'a> {
    pub container_id: &'a str,
    pub action: &'a str,
}

impl<'a> InternalEvent for DockerContainerEventReceived<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "received one container event.",
            container_id = %self.container_id,
            action = %self.action
        );
    }

    fn emit_metrics(&self) {
        counter!("container_events_processed", 1,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
    }
}

#[derive(Debug)]
pub struct DockerContainerWatch<'a> {
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerContainerWatch<'a> {
    fn emit_logs(&self) {
        info!(
            message = "started watching for logs of container.",
            container_id = %self.container_id,
        );
    }

    fn emit_metrics(&self) {
        counter!("containers_watched", 1,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
    }
}

#[derive(Debug)]
pub struct DockerContainerUnwatch<'a> {
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerContainerUnwatch<'a> {
    fn emit_logs(&self) {
        info!(
            message = "stoped watching for logs of container.",
            container_id = %self.container_id,
        );
    }

    fn emit_metrics(&self) {
        counter!("containers_unwatched", 1,
                 "component_kind" => "source",
                 "component_name" => "docker",
        );
    }
}
