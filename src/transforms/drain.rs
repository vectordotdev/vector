use std::pin::Pin;

use async_stream::stream;
use drain_log::{Config as DrainLogConfig, Matcher, Template};
use futures::{Stream, StreamExt};
use snafu::Snafu;
use vector_lib::{
    config::clone_input_definitions,
    configurable::configurable_component,
    lookup::{OwnedTargetPath, lookup_v2::OptionalValuePath, owned_value_path},
};

use crate::{
    config::{
        DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput,
    },
    event::{Event, Value},
    schema,
    transforms::{TaskTransform, Transform},
};

const PARAM_STR: &str = "<*>";

const fn default_tree_depth() -> usize {
    4
}

const fn default_max_node_children() -> usize {
    100
}

const fn default_merge_threshold() -> f64 {
    0.4
}

const fn default_max_bytes() -> usize {
    8192
}

const fn default_max_tokens() -> usize {
    256
}

fn default_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("message"))
}

fn default_template_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("drain_template"))
}

/// Configuration for the `drain` transform.
#[configurable_component(transform(
    "drain",
    "Cluster log events with the Drain algorithm and annotate each event with the derived template."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DrainConfig {
    /// The log field to read text from before passing it through the Drain
    /// algorithm.
    ///
    /// If the field is missing or not a string, the event is forwarded without
    /// annotation.
    #[serde(default = "default_field")]
    #[configurable(metadata(docs::examples = "message", docs::examples = "body"))]
    pub field: OptionalValuePath,

    /// The log field to write the derived template string to.
    ///
    /// Set to an empty string to disable writing the template (the tree still
    /// trains on every event). To target an OpenTelemetry-style attribute name
    /// containing dots — e.g. `log.record.template` — set this to the quoted
    /// path `"log.record.template"` so the dots are treated as part of a
    /// single field name rather than a nested path.
    #[serde(default = "default_template_field")]
    #[configurable(metadata(docs::examples = "drain_template"))]
    pub template_field: OptionalValuePath,

    /// Maximum depth of the Drain parse tree (called `depth` in the Drain
    /// paper). Higher values produce more specific templates. Minimum: 3.
    #[serde(default = "default_tree_depth")]
    pub tree_depth: usize,

    /// Minimum fraction of tokens that must match an existing cluster template
    /// for a log line to be merged into it rather than forming a new cluster
    /// (called `st` in the Drain paper). Range: 0.0–1.0.
    #[serde(default = "default_merge_threshold")]
    pub merge_threshold: f64,

    /// Maximum children per internal parse tree node (called `maxChild` in the
    /// Drain paper). Bounds memory on high-cardinality token positions.
    #[serde(default = "default_max_node_children")]
    pub max_node_children: usize,

    /// Maximum number of clusters to track. Once the limit is reached, the
    /// least-recently-used cluster is evicted to make room for a new one,
    /// so the matcher continues to adapt to drifting log vocabularies on
    /// long-running pipelines without unbounded memory growth. `0` means
    /// unlimited (no eviction).
    ///
    /// Pick a value comfortably above the steady-state pattern count for
    /// your workload so genuinely useful templates are not churned out by
    /// transient noise.
    #[serde(default)]
    pub max_clusters: usize,

    /// Maximum byte length of a single log line to consider. Lines longer than
    /// this are skipped (no annotation).
    #[serde(default = "default_max_bytes")]
    pub max_bytes: usize,

    /// Maximum number of tokens per line. Lines exceeding this are skipped.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Additional token delimiters beyond whitespace (for example `[",", ":"]`).
    #[serde(default)]
    pub extra_delimiters: Vec<String>,

    /// Pre-known template strings to train on at startup. Improves template
    /// stability across restarts for known log patterns.
    #[serde(default)]
    pub seed_templates: Vec<String>,

    /// Raw example log lines to train on at startup. Drain derives templates
    /// from these lines itself.
    #[serde(default)]
    pub seed_logs: Vec<String>,

    /// Number of distinct clusters that must be observed before annotation is
    /// enabled. During warmup, events pass through unannotated while the tree
    /// keeps training. `0` (default) disables warmup suppression.
    #[serde(default)]
    pub warmup_min_clusters: usize,
}

impl Default for DrainConfig {
    fn default() -> Self {
        Self {
            field: default_field(),
            template_field: default_template_field(),
            tree_depth: default_tree_depth(),
            merge_threshold: default_merge_threshold(),
            max_node_children: default_max_node_children(),
            max_clusters: 0,
            max_bytes: default_max_bytes(),
            max_tokens: default_max_tokens(),
            extra_delimiters: Vec::new(),
            seed_templates: Vec::new(),
            seed_logs: Vec::new(),
            warmup_min_clusters: 0,
        }
    }
}

impl_generate_config_from_default!(DrainConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "drain")]
impl TransformConfig for DrainConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::event_task(Drain::new(self)?))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        // The transform only adds a string field; pass the input schema through.
        vec![TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        )]
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("failed to build drain matcher: {source}"))]
    Build { source: drain_log::Error },
}

pub struct Drain {
    /// Pre-resolved event-root path for the source field, or `None` when the
    /// user supplied an empty path (effectively disabling annotation).
    field: Option<OwnedTargetPath>,
    /// Pre-resolved event-root path for the destination field. `None` means
    /// "train but don't write" (the matcher still updates its tree).
    template_field: Option<OwnedTargetPath>,
    warmup_min_clusters: usize,
    warmed_up: bool,
    matcher: Matcher,
}

impl Drain {
    pub fn new(cfg: &DrainConfig) -> crate::Result<Self> {
        let drain_cfg = DrainLogConfig::builder()
            .depth(cfg.tree_depth)
            .similarity_threshold(cfg.merge_threshold)
            .max_children(cfg.max_node_children)
            .max_clusters(cfg.max_clusters)
            .max_bytes(cfg.max_bytes)
            .max_tokens(cfg.max_tokens)
            .extra_delimiters(cfg.extra_delimiters.clone())
            .build();

        let mut matcher = drain_log::train(&[], drain_cfg)
            .map_err(|source| Box::new(BuildError::Build { source }))?;

        for tmpl in &cfg.seed_templates {
            if tmpl.trim().is_empty() {
                continue;
            }
            if let Err(error) = matcher.add_log_message(tmpl) {
                warn!(
                    message = "Failed to seed drain template, skipping.",
                    template = %tmpl,
                    %error,
                );
            }
        }
        for line in &cfg.seed_logs {
            if line.trim().is_empty() {
                continue;
            }
            if let Err(error) = matcher.add_log_message(line) {
                warn!(
                    message = "Failed to seed drain log line, skipping.",
                    line = %line,
                    %error,
                );
            }
        }

        let warmed_up = cfg.warmup_min_clusters == 0
            || matcher.cluster_count() >= cfg.warmup_min_clusters;

        let field = cfg.field.path.clone().map(OwnedTargetPath::event);
        let template_field = cfg.template_field.path.clone().map(OwnedTargetPath::event);

        Ok(Self {
            field,
            template_field,
            warmup_min_clusters: cfg.warmup_min_clusters,
            warmed_up,
            matcher,
        })
    }

    fn transform_one(&mut self, mut event: Event) -> Event {
        let Some(field_path) = self.field.as_ref() else {
            return event;
        };

        let log = match &mut event {
            Event::Log(log) => log,
            _ => return event,
        };

        let text = match log.get(field_path) {
            Some(Value::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => return event,
        };

        if text.is_empty() {
            return event;
        }

        let template = match self.matcher.add_log_message(&text) {
            Ok(t) => t,
            Err(drain_log::Error::LineTooLong { .. }) => {
                // The line exceeds the configured max_bytes. Forward without
                // annotation; logging per-event would be too noisy.
                return event;
            }
            Err(error) => {
                debug!(message = "Drain training failed; skipping annotation.", %error);
                return event;
            }
        };

        if !self.warmed_up {
            if self.matcher.cluster_count() >= self.warmup_min_clusters {
                self.warmed_up = true;
            } else {
                return event;
            }
        }

        if let Some(template_path) = self.template_field.as_ref() {
            log.insert(template_path, Value::from(template_to_string(&template)));
        }
        event
    }
}

fn template_to_string(t: &Template) -> String {
    let mut out = String::new();
    let mut tok_idx = 0;
    for i in 0..t.token_count() {
        if i > 0 {
            out.push(' ');
        }
        if t.is_param(i) {
            out.push_str(PARAM_STR);
        } else {
            out.push_str(&t.tokens()[tok_idx]);
            tok_idx += 1;
        }
    }
    out
}

impl TaskTransform<Event> for Drain {
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = *self;
        Box::pin(stream! {
            while let Some(event) = input_rx.next().await {
                yield inner.transform_one(event);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    use super::*;
    use crate::{
        event::LogEvent,
        test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DrainConfig>();
    }

    fn log(message: &str) -> Event {
        Event::Log(LogEvent::from(message))
    }

    #[tokio::test]
    async fn annotates_with_template() {
        let config: DrainConfig = toml::from_str(indoc! {r#"
            seed_templates = [
              "user <*> logged in from <*>",
            ]
        "#})
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(2);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            tx.send(log("user alice logged in from 10.0.0.1"))
                .await
                .unwrap();
            tx.send(log("user bob logged in from 192.168.1.1"))
                .await
                .unwrap();

            let first = out.recv().await.unwrap();
            let second = out.recv().await.unwrap();

            let first = first.as_log();
            let second = second.as_log();

            let t1 = first
                .get("drain_template")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .expect("first event should have drain_template");
            let t2 = second
                .get("drain_template")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .expect("second event should have drain_template");

            assert!(
                t1.contains("user") && t1.contains("logged in from"),
                "template should retain anchor tokens, got {t1}"
            );
            assert_eq!(
                t1, t2,
                "both events should resolve to the same template"
            );
            assert!(t1.contains("<*>"));

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn warmup_suppresses_annotation() {
        // warmup requires 3 clusters before annotation kicks in. The first
        // two distinct lines train but don't write the template. The third
        // (a third distinct cluster) triggers warmup completion and gets
        // annotated.
        let config: DrainConfig = toml::from_str(indoc! {r#"
            warmup_min_clusters = 3
        "#})
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(3);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            tx.send(log("alpha event happened")).await.unwrap();
            tx.send(log("connection refused 5")).await.unwrap();
            tx.send(log("disk usage 99 percent")).await.unwrap();

            let first = out.recv().await.unwrap();
            let second = out.recv().await.unwrap();
            let third = out.recv().await.unwrap();

            assert!(
                first.as_log().get("drain_template").is_none(),
                "first event should be unannotated during warmup"
            );
            assert!(
                second.as_log().get("drain_template").is_none(),
                "second event should be unannotated during warmup"
            );
            assert!(
                third.as_log().get("drain_template").is_some(),
                "third event should be annotated once warmup completes"
            );

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn missing_field_passes_through() {
        let config: DrainConfig = toml::from_str(indoc! {r#"
            field = "body"
        "#})
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            // The event has a `message` field but no `body`, so the transform
            // should leave it unannotated.
            tx.send(log("nothing here")).await.unwrap();

            let event = out.recv().await.unwrap();
            assert!(event.as_log().get("drain_template").is_none());

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn custom_template_field() {
        let config: DrainConfig = toml::from_str(indoc! {r#"
            template_field = "tpl"
            seed_templates = ["request <*> handled"]
        "#})
        .unwrap();

        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            tx.send(log("request 42 handled")).await.unwrap();
            let event = out.recv().await.unwrap();

            assert!(event.as_log().get("drain_template").is_none());
            let tpl = event
                .as_log()
                .get("tpl")
                .and_then(|v| v.as_str())
                .expect("custom template field should be set");
            assert!(tpl.contains("request") && tpl.contains("handled"));

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

}
