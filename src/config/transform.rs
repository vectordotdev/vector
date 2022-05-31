use std::collections::HashSet;

use component::ComponentDescription;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::transform::TransformConfig;

use super::{component, ComponentKey};

#[derive(Deserialize, Serialize, Debug)]
pub struct TransformOuter<T> {
    #[serde(default = "Default::default")] // https://github.com/serde-rs/serde/issues/1541
    pub inputs: Vec<T>,
    #[serde(flatten)]
    pub inner: Box<dyn TransformConfig>,
}

impl<T> TransformOuter<T> {
    #[cfg(feature = "enterprise")]
    pub(super) fn new(inputs: Vec<T>, transform: impl TransformConfig + 'static) -> Self {
        TransformOuter {
            inputs,
            inner: Box::new(transform),
        }
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> TransformOuter<U> {
        let inputs = self.inputs.iter().map(f).collect();
        self.with_inputs(inputs)
    }

    pub(crate) fn with_inputs<U>(self, inputs: Vec<U>) -> TransformOuter<U> {
        TransformOuter {
            inputs,
            inner: self.inner,
        }
    }
}

impl TransformOuter<String> {
    pub(crate) fn expand(
        mut self,
        key: ComponentKey,
        parent_types: &HashSet<&'static str>,
        transforms: &mut IndexMap<ComponentKey, TransformOuter<String>>,
        expansions: &mut IndexMap<ComponentKey, Vec<ComponentKey>>,
    ) -> Result<(), String> {
        if !self.inner.nestable(parent_types) {
            return Err(format!(
                "the component {} cannot be nested in {:?}",
                self.inner.transform_type(),
                parent_types
            ));
        }

        let expansion = self
            .inner
            .expand(&key, &self.inputs)
            .map_err(|err| format!("failed to expand transform '{}': {}", key, err))?;

        let mut ptypes = parent_types.clone();
        ptypes.insert(self.inner.transform_type());

        if let Some(inner_topology) = expansion {
            let mut children = Vec::new();

            expansions.insert(
                key.clone(),
                inner_topology
                    .outputs()
                    .into_iter()
                    .map(ComponentKey::from)
                    .collect(),
            );

            for (inner_name, inner_transform) in inner_topology.inner {
                let child = TransformOuter {
                    inputs: inner_transform.inputs,
                    inner: inner_transform.inner,
                };
                children.push(inner_name.clone());
                transforms.insert(inner_name, child);
            }
        } else {
            transforms.insert(key, self);
        }
        Ok(())
    }
}

pub type TransformDescription = ComponentDescription<Box<dyn TransformConfig>>;

inventory::collect!(TransformDescription);
