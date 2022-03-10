use std::collections::HashSet;

use component::ComponentDescription;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::transform::{ExpandType, TransformConfig};

use super::{component, ComponentKey};
use crate::transforms::noop::Noop;

#[derive(Deserialize, Serialize, Debug)]
pub struct TransformOuter<T> {
    #[serde(default = "Default::default")] // https://github.com/serde-rs/serde/issues/1541
    pub inputs: Vec<T>,
    #[serde(flatten)]
    pub inner: Box<dyn TransformConfig>,
}

impl<T> TransformOuter<T> {
    #[cfg(test)]
    pub(crate) fn new(transform: impl TransformConfig + 'static) -> Self {
        Self {
            inputs: vec![],
            inner: Box::new(transform),
        }
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> TransformOuter<U> {
        let inputs = self.inputs.iter().map(f).collect();
        self.with_inputs(inputs)
    }

    pub(super) fn with_inputs<U>(self, inputs: Vec<U>) -> TransformOuter<U> {
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
            .expand()
            .map_err(|err| format!("failed to expand transform '{}': {}", key, err))?;

        let mut ptypes = parent_types.clone();
        ptypes.insert(self.inner.transform_type());

        if let Some((expanded, expand_type)) = expansion {
            let mut children = Vec::new();
            let mut inputs = self.inputs.clone();

            for (name, content) in expanded {
                let full_name = key.join(name);

                let child = TransformOuter {
                    inputs,
                    inner: content,
                };
                child.expand(full_name.clone(), &ptypes, transforms, expansions)?;
                children.push(full_name.clone());

                inputs = match expand_type {
                    ExpandType::Parallel { .. } => self.inputs.clone(),
                    ExpandType::Serial { .. } => vec![full_name.to_string()],
                }
            }

            if matches!(expand_type, ExpandType::Parallel { aggregates: true }) {
                transforms.insert(
                    key.clone(),
                    TransformOuter {
                        inputs: children.iter().map(ToString::to_string).collect(),
                        inner: Box::new(Noop),
                    },
                );
                children.push(key.clone());
            } else if matches!(expand_type, ExpandType::Serial { alias: true }) {
                transforms.insert(
                    key.clone(),
                    TransformOuter {
                        inputs,
                        inner: Box::new(Noop),
                    },
                );
                children.push(key.clone());
            }

            expansions.insert(key.clone(), children);
        } else {
            transforms.insert(key, self);
        }
        Ok(())
    }
}

pub type TransformDescription = ComponentDescription<Box<dyn TransformConfig>>;

inventory::collect!(TransformDescription);
