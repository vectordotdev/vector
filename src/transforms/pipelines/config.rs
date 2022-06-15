use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use vector_core::{
    config::Input,
    event::{Event, EventArray, EventContainer},
    schema,
    transform::{
        InnerTopology, InnerTopologyTransform, SyncTransform, Transform, TransformContext,
        TransformOutputsBuf,
    },
};

use crate::{
    conditions::{AnyCondition, Condition},
    config::{ComponentKey, DataType, Output, TransformConfig},
};

// 64 is a lowish number and arbitrarily chosen: there is no magic to this magic
// constant.
const INTERIOR_BUFFER_SIZE: usize = 64;

//------------------------------------------------------------------------------

/// This represents the configuration of a single pipeline, not the pipelines transform
/// itself, which can contain multiple individual pipelines
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct PipelineConfig {
    name: String,
    filter: Option<AnyCondition>,
    #[serde(default)]
    transforms: Vec<Box<dyn TransformConfig>>,
}

#[cfg(test)]
impl PipelineConfig {
    #[allow(dead_code)] // for some small subset of feature flags this code is dead
    pub(crate) fn transforms(&self) -> &Vec<Box<dyn TransformConfig>> {
        &self.transforms
    }
}

impl Clone for PipelineConfig {
    fn clone(&self) -> Self {
        // This is a hack around the issue of cloning
        // trait objects. So instead to clone the config
        // we first serialize it into JSON, then back from
        // JSON. Originally we used TOML here but TOML does not
        // support serializing `None`.
        let json = serde_json::to_value(self).unwrap();
        serde_json::from_value(json).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipeline")]
impl TransformConfig for PipelineConfig {
    async fn build(&self, ctx: &TransformContext) -> crate::Result<Transform> {
        let condition = match &self.filter {
            Some(config) => Some(config.build(&ctx.enrichment_tables)?),
            None => None,
        };

        // Setup the interior transform chain in this pipeline.
        //
        // NOTE: As this pipeline is not expanding to separate transforms we
        // lose validation that happens at the topology layer (e.g. making sure
        // input/output types of connected transforms are compatible). This is
        // an unfortunate consequence of having a "topology in a topology".
        //
        // In the future, it would be great to decouple that type of validation
        // from the physical task boundaries. You could imagine a future where
        // we replace transform expansion with the ability to return different
        // types of compound transforms directly from TransformConfig::build,
        // and those know how to play nice with a subsequent graph-based
        // validation scheme.
        //
        // For now, considering that most everything is log to log with regard
        // to pipelines we avoid adding duplicate validation here in favor of
        // future work.
        if self.transforms.is_empty() {
            return Err(format!("empty pipeline: {}", self.name).into());
        }
        // Today we make the assumption that to be a valid pipeline transform
        // the transform CANNOT have named outputs. This assumption might break
        // in the future so, to avoid panics, we instead make building a
        // pipeline with such transforms an error.
        for transform in &self.transforms {
            if transform.outputs(&ctx.merged_schema_definition).len() > 1 {
                return Err(format!(
                    "pipeline {} has transform of type {} with a named output, unsupported",
                    self.name,
                    transform.transform_type()
                )
                .into());
            }
        }

        let mut transforms = Vec::with_capacity(self.transforms.len());
        for config in &self.transforms {
            let transform = match config.build(ctx).await? {
                Transform::Function(transform) => Box::new(transform),
                Transform::Synchronous(transform) => transform,
                _ => return Err(format!("non-sync transform in pipeline: {:?}", config).into()),
            };
            transforms.push(transform);
        }

        let buf_in = TransformOutputsBuf::new_with_capacity(
            vec![Output::default(DataType::all())],
            INTERIOR_BUFFER_SIZE,
        );
        let buf_out = buf_in.clone();
        Ok(Transform::Synchronous(Box::new(Pipeline {
            condition,
            transforms,
            buf_in,
            buf_out,
        })))
    }

    fn input(&self) -> Input {
        if let Some(transform) = self.transforms.first() {
            transform.input()
        } else {
            panic!("pipeline {} does not have transforms", self.name)
        }
    }

    fn outputs(&self, schema: &schema::Definition) -> Vec<Output> {
        if let Some(transform) = self.transforms.last() {
            transform.outputs(schema)
        } else {
            panic!("pipeline {} does not have transforms", self.name)
        }
    }

    fn transform_type(&self) -> &'static str {
        // NOTE It'd be nice to avoid this boilerplate for transforms that are
        // not meant to be directly user-configurable. In this case, we're
        // really only implementing TransformConfig because we're ultimately
        // returning this from an expansion, so it's likely this probably goes
        // away when expansion goes away.
        "pipeline"
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

impl PipelineConfig {
    pub(super) fn expand(
        &mut self,
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        let mut result = InnerTopology::default();

        result.inner.insert(
            name.clone(),
            InnerTopologyTransform {
                inputs: inputs.to_vec(),
                inner: Box::new(self.clone()),
            },
        );
        result
            .outputs
            .push((name.clone(), vec![Output::default(DataType::all())]));
        Ok(Some(result))
    }
}

#[derive(Clone)]
struct Pipeline {
    condition: Option<Condition>,
    transforms: Vec<Box<dyn SyncTransform>>,
    buf_in: TransformOutputsBuf,
    buf_out: TransformOutputsBuf,
}

impl SyncTransform for Pipeline {
    fn transform(&mut self, _event: Event, _output: &mut TransformOutputsBuf) {
        // NOTE This is a bit of a wart in the SyncTransform API. We could
        // consider splitting out another BatchTransform variant for
        // transform_all and implementing the trait for SyncTransform instead.
        unimplemented!()
    }

    fn transform_all(&mut self, events: EventArray, output: &mut TransformOutputsBuf) {
        // A `Pipeline` is a compound of sub-transforms. That is, it's a
        // transform that runs other transforms. To achieve this we gate all
        // incoming Events by whether they match the pipeline condition or not
        // and, if they do not, immediately output them. If the Event does match
        // our condition we queue it up for further processing.
        //
        // Here our queue is the TransformOutputsBuf. In the next chunk of code
        // that follows we do the aforementioned filtering and push into
        // `self.buf_out`.
        let ev_container = events.into_events();
        if let Some(condition) = &self.condition {
            for event in ev_container {
                let (result, event) = condition.check(event);
                if result {
                    self.buf_out.push(event);
                } else {
                    output.push(event);
                }
            }
        } else {
            self.buf_out.extend(ev_container);
        }

        // `buf_out` is now primed with Events. Note that the struct also has a
        // `buf_in`. The pipeline now runs each sub-transform in serial,
        // flip-flopping the in and out TransformOutputsBuf so that the input of
        // one transform becomes the output of the next, after it has been
        // emptied. Once all the transforms are run, the Events in `buf_out` are
        // emitted to `output`. When this function runs again `buf_out` is
        // empty, `buf_in` is empty and the process is ready to begin again.
        for transform in &mut self.transforms {
            std::mem::swap(&mut self.buf_out, &mut self.buf_in);
            for event in self.buf_in.drain() {
                transform.transform(event, &mut self.buf_out);
            }
        }
        output.extend(self.buf_out.drain());
    }
}

//------------------------------------------------------------------------------

/// This represent an ordered list of pipelines depending on the event type.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct EventTypeConfig(Vec<PipelineConfig>);

impl AsRef<Vec<PipelineConfig>> for EventTypeConfig {
    fn as_ref(&self) -> &Vec<PipelineConfig> {
        &self.0
    }
}

impl EventTypeConfig {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn validate_nesting(&self, parents: &HashSet<&'static str>) -> Result<(), String> {
        for (pipeline_index, pipeline) in self.0.iter().enumerate() {
            let pipeline_name = pipeline.name.as_str();
            for (transform_index, transform) in pipeline.transforms.iter().enumerate() {
                if !transform.nestable(parents) {
                    return Err(format!(
                        "the transform {} in pipeline {:?} (at index {}) cannot be nested in {:?}",
                        transform_index, pipeline_name, pipeline_index, parents
                    ));
                }
            }
        }
        Ok(())
    }
}

impl EventTypeConfig {
    /// Expand sub-pipelines configurations, preserving user defined order
    ///
    /// This function expands the sub-pipelines according to the order passed by
    /// the user, or, absent an explicit order, by the position of the
    /// sub-pipeline in the configuration file.
    pub(super) fn expand(
        &mut self,
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        let mut result = InnerTopology::default();
        let mut next_inputs = inputs.to_vec();
        for (pipeline_index, pipeline_config) in self.0.iter_mut().enumerate() {
            let pipeline_name = name.join(pipeline_index);
            let topology = pipeline_config
                .expand(&pipeline_name, &next_inputs)?
                .ok_or_else(|| {
                    format!(
                        "Unable to expand pipeline {:?} ({:?})",
                        pipeline_config.name, pipeline_name
                    )
                })?;
            result.inner.extend(topology.inner.into_iter());
            result.outputs = topology.outputs;
            next_inputs = result.outputs();
        }
        //
        Ok(Some(result))
    }
}
