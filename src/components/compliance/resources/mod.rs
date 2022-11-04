mod http;

use tokio::sync::mpsc;
use vector_core::{config::DataType, event::Event};

pub use self::http::HttpConfig;

use super::sync::{Configured, ExternalResourceCoordinator, WaitHandle};

#[derive(Clone, Copy)]
pub enum Codec {
    JSON,
    Syslog,
    Plaintext,
}

impl Codec {
    pub fn allowed_event_types(self) -> DataType {
        // TODO: Actually derive this from the supported data types declared for codecs defined in
        // the `codecs` crate.
        match self {
            Self::JSON => DataType::Log,
            Self::Syslog => DataType::Log,
            Self::Plaintext => DataType::Log,
        }
    }
}

pub struct Payload {
    codec: Codec,
    event_types: DataType,
}

impl Payload {
    pub fn from_codec(codec: Codec) -> Self {
        Self {
            codec,
            event_types: codec.allowed_event_types(),
        }
    }
}

/// Direction that the resource is operating in.
pub enum ResourceDirection {
    /// Resource will have the component pull data from it, or pull data from the component.
    ///
    /// For a source, where an external resource functions in "input" mode, this would be the
    /// equivalent of the source calling out to the external resource (HTTP server, Kafka cluster,
    /// etc) and asking for data, or expecting it to be returned in the response.
    ///
    /// For a sink, where an external resource functions in "output" mode, this would be the
    /// equivalent of the sink exposing a network endpoint and having the external resource be
    /// responsible for connecting to the endpoint to grab the data.
    Pull,

    /// Resource will push data to the component, or have data pushed to it from the component.
    ///
    /// For a source, where an external resource functions in "input" mode, this would be the
    /// equivalent of the source waiting for data to be sent to either, whether it's listening on a
    /// network endpoint for traffic, or polling files on disks for updates, and the external
    /// resource would be responsible for initiating that communication, or writing to those files.
    ///
    /// For a sink, where an external resource functions in "output" mode, this would be the
    /// equivalent of the sink pushing its data to a network endpoint, or writing data to files,
    /// where the external resource would be responsible for aggregating that data, or read from
    /// those files.
    Push,
}

/// A resource definition.
///
/// Resource definitions uniquely identify the resource, such as HTTP, or files, and so on. These
/// definitions generally include the bare minimum amount of information to allow the component
/// validation runner to create an instance of them, such as spawning an HTTP server if a source has
/// specified an HTTP resource in the "pull" direction.
pub enum ResourceDefinition {
    Http(HttpConfig),
}

/// An external resource associated with a component.
///
/// External resources represent the hypothetical location where, depending on whether the component
/// is a source or sink, data would be generated from or collected at. This includes things like
/// network endpoints (raw sockets, HTTP servers, etc) as well as files on disk, and more. In other
/// words, an external resource is a data dependency associated with the component, whether the
/// component depends on data from the external resource, or the external resource depends on data
/// from the component.
///
/// An external resource includes a direction -- push or pull -- as well as the fundamental
/// definition of the resource, such as HTTP or file. The component type is used to further refine
/// the direction of the resource, such that a "pull" resource used with a source implies the source
/// will pull data from the external resource, whereas a "pull" resource used with a sink implies
/// the external resource must pull the data from the sink.
pub struct ExternalResource {
    direction: ResourceDirection,
    definition: ResourceDefinition,
    payload: Payload,
}

impl ExternalResource {
    /// Creates a new `ExternalResource` based on the given `direction`, `definition`, and `codec`.
    pub fn new(direction: ResourceDirection, definition: ResourceDefinition, codec: Codec) -> Self {
        Self {
            direction,
            definition,
            payload: Payload::from_codec(codec),
        }
    }

    /// Spawns this resource for use as an input to a source.
    pub fn spawn_as_input(
        self,
        input_rx: mpsc::Receiver<Event>,
        resource_coordinator: &ExternalResourceCoordinator<Configured>,
        resource_shutdown_handle: WaitHandle,
    ) {
        match self.definition {
            ResourceDefinition::Http(http_config) => http_config.spawn_as_input(
                self.direction,
                input_rx,
                resource_coordinator,
                resource_shutdown_handle,
            ),
        }
    }

    /// Spawns this resource for use as an output for a sink.
    pub fn spawn_as_output(
        self,
        output_tx: mpsc::Sender<Event>,
        resource_coordinator: &ExternalResourceCoordinator<Configured>,
        resource_shutdown_handle: WaitHandle,
    ) {
        match self.definition {
            ResourceDefinition::Http(http_config) => http_config.spawn_as_output(
                self.direction,
                output_tx,
                resource_coordinator,
                resource_shutdown_handle,
            ),
        }
    }
}
