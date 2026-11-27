use vector_lib::configurable::configurable_component;

/// Configuration for Elasticsearch API version.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
#[derive(Default)]
pub enum ElasticsearchApiVersion {
    /// Automatically select the API version.
    ///
    /// The `opensearch` sink selects an OpenSearch-compatible API version without querying the server.
    /// The `elasticsearch` sink detects the version from the server.
    ///
    /// If the [cluster state version endpoint][es_version] isn't reachable, a warning is logged to
    /// stdout, and the version is assumed to be V6 if the `suppress_type_name` option is set to
    /// `true`. Otherwise, the version is assumed to be V8. In the future, the sink instead
    /// returns an error during configuration parsing, since a wrongly assumed version could lead to
    /// incorrect API calls.
    ///
    /// [es_version]: https://www.elastic.co/guide/en/elasticsearch/reference/current/cluster-state.html#cluster-state-api-path-params
    #[default]
    Auto,
    /// Use the Elasticsearch 6.x API.
    V6,
    /// Use the Elasticsearch 7.x-compatible API, including OpenSearch.
    V7,
    /// Use the Elasticsearch 8.x API.
    V8,
}
