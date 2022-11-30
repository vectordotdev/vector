use std::collections::HashMap;

use futures::future::FutureExt;
use vector_config::configurable_component;
use vector_core::config::{LogNamespace, Output};

use super::{healthcheck::healthcheck, sink::LokiSink};
use crate::{
    codecs::EncodingConfig,
    config::{DataType, GenerateConfig, Input, SourceContext},
    http::{Auth, HttpClient, MaybeAuth},
    template::Template,
    tls::{TlsConfig, TlsSettings},
};
use crate::config::SourceConfig;

fn default_loki_path() -> String {
    "/loki/api/v1/tail".to_string()
}

/// Configuration for the `loki` sink.
#[configurable_component(source("loki"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LokiSourceConfig {
    /// The base URL of the Loki instance.
    ///
    /// Vector will append the value of `path` to this.
    pub endpoint: String,

    /// The path to use in the URL of the Loki instance.
    ///
    /// By default, `"/loki/api/v1/tail"` is used.
    #[serde(default = "default_loki_path")]
    pub path: String,

    /// A set of labels that are attached to each batch of events.
    ///
    /// Both keys and values are templateable, which enables you to attach dynamic labels to events.
    ///
    /// Labels can be suffixed with a “*” to allow the expansion of objects into multiple labels,
    /// see “How it works” for more information.
    ///
    /// Note: If the set of labels has high cardinality, this can cause drastic performance issues
    /// with Loki. To prevent this from happening, reduce the number of unique label keys and
    /// values.
    pub labels: HashMap<Template, Template>,

    /// Whether or not to delete fields from the event when they are used as labels.
    #[serde(default = "crate::serde::default_false")]
    pub remove_label_fields: bool,

    /// Whether or not to remove the timestamp from the event payload.
    ///
    /// The timestamp will still be sent as event metadata for Loki to use for indexing.
    #[serde(default = "crate::serde::default_true")]
    pub remove_timestamp: bool,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,
}

impl GenerateConfig for LokiSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "ws://localhost:3100"
            labels = {}"#,
        )
            .unwrap()
    }
}

impl LokiSourceConfig {
    pub(super) fn build_client(&self, cx: &SourceContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, &cx.proxy)?;
        Ok(client)
    }
}

#[async_trait::async_trait]
impl SourceConfig for LokiSourceConfig {
    async fn build(
        &self,
        cx: SourceContext,
    ) -> crate::Result<vector_core::source::Source> {
        if self.labels.is_empty() {
            return Err("`labels` must include at least one label.".into());
        }

        for label in self.labels.keys() {
            if !valid_label_name(label) {
                return Err(format!("Invalid label name {:?}", label.get_ref()).into());
            }
        }

        let client = self.build_client(&cx)?;

        //let healthcheck = healthcheck(config, client).boxed();

        Ok(Box::pin(super::source::loki_source(
            &self,
            client,
            cx.shutdown,
            cx.out,
        )))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub fn valid_label_name(label: &Template) -> bool {
    label.is_dynamic() || {
        // Loki follows prometheus on this https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
        // Although that isn't explicitly said anywhere besides what's in the code.
        // The closest mention is in section about Parser Expression https://grafana.com/docs/loki/latest/logql/
        //
        // [a-zA-Z_][a-zA-Z0-9_]*
        //
        // '*' symbol at the end of the label name will be treated as a prefix for
        // underlying object keys.
        let mut label_trim = label.get_ref().trim();
        if let Some(without_opening_end) = label_trim.strip_suffix('*') {
            label_trim = without_opening_end
        }

        let mut label_chars = label_trim.chars();
        if let Some(ch) = label_chars.next() {
            (ch.is_ascii_alphabetic() || ch == '_')
                && label_chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::valid_label_name;

    #[test]
    fn valid_label_names() {
        assert!(valid_label_name(&"name".try_into().unwrap()));
        assert!(valid_label_name(&" name ".try_into().unwrap()));
        assert!(valid_label_name(&"bee_bop".try_into().unwrap()));
        assert!(valid_label_name(&"a09b".try_into().unwrap()));
        assert!(valid_label_name(&"abc_*".try_into().unwrap()));
        assert!(valid_label_name(&"_*".try_into().unwrap()));

        assert!(!valid_label_name(&"0ab".try_into().unwrap()));
        assert!(!valid_label_name(&"*".try_into().unwrap()));
        assert!(!valid_label_name(&"".try_into().unwrap()));
        assert!(!valid_label_name(&" ".try_into().unwrap()));

        assert!(valid_label_name(&"{{field}}".try_into().unwrap()));
    }
}
