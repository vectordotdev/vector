pub mod acknowledgements;
pub mod request;
pub mod response;
pub mod service;
pub mod util;

pub use util::*;
use vector_lib::configurable::configurable_component;

pub(super) const SOURCE_FIELD: &str = "source";
pub(super) const SOURCETYPE_FIELD: &str = "sourcetype";
pub(super) const INDEX_FIELD: &str = "index";
pub(super) const HOST_FIELD: &str = "host";
pub(super) const AUTO_EXTRACT_TIMESTAMP_FIELD: &str = "auto_extract_timestamp";

/// Splunk HEC endpoint configuration.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointTarget {
    /// Events are sent to the [raw endpoint][raw_endpoint_docs].
    ///
    /// When the raw endpoint is used, configured [event metadata][event_metadata_docs] is sent as
    /// query parameters on the request, except for the `timestamp` field.
    ///
    /// [raw_endpoint_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/RESTREF/RESTinput#services.2Fcollector.2Fraw
    /// [event_metadata_docs]: https://docs.splunk.com/Documentation/Splunk/latest/Data/FormateventsforHTTPEventCollector#Event_metadata
    Raw,

    /// Events are sent to the [event endpoint][event_endpoint_docs].
    ///
    /// When the event endpoint is used, configured [event metadata][event_metadata_docs] is sent
    /// directly with each event.
    ///
    /// [event_endpoint_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/RESTREF/RESTinput#services.2Fcollector.2Fevent
    /// [event_metadata_docs]: https://docs.splunk.com/Documentation/Splunk/latest/Data/FormateventsforHTTPEventCollector#Event_metadata
    #[default]
    Event,
}
