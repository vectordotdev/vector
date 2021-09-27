use super::{
    format::{deserialize, Format},
    ComponentKey, TransformOuter,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct Pipelines(IndexMap<String, Pipeline>);

impl From<IndexMap<String, Pipeline>> for Pipelines {
    fn from(value: IndexMap<String, Pipeline>) -> Self {
        Self(value)
    }
}

impl Pipelines {
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub const fn inner(&self) -> &IndexMap<String, Pipeline> {
        &self.0
    }

    pub fn load_from_paths(paths: &[PathBuf]) -> Result<Pipelines, Vec<String>> {
        let mut index: IndexMap<String, Pipeline> = IndexMap::new();
        let mut errors: Vec<String> = Vec::new();
        for folder in paths {
            match Self::load_from_folder(folder) {
                Ok(result) => {
                    for (key, value) in result.into_iter() {
                        index.insert(key, value);
                    }
                }
                Err(result) => {
                    for err in result.into_iter() {
                        errors.push(err);
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(Self::from(index))
        } else {
            Err(errors)
        }
    }

    fn load_from_folder(folder: &Path) -> Result<IndexMap<String, Pipeline>, Vec<String>> {
        let mut index = IndexMap::new();
        let mut errors = Vec::new();
        fs::read_dir(folder)
            .map_err(|err| {
                vec![format!(
                    "Could not list folder content: {:?}, {}",
                    folder, err
                )]
            })?
            .filter_map(|entry| match entry {
                Ok(item) => {
                    let path = item.path();
                    let format = match Format::from_path(&path) {
                        Ok(value) => value,
                        Err(path) => {
                            debug!("Could not get format for {:?}.", path);
                            return None;
                        }
                    };
                    if path.is_file() {
                        Some(Pipeline::load_from_file(&path, format))
                    } else {
                        None
                    }
                }
                Err(err) => Some(Err(err.to_string())),
            })
            .for_each(|res| match res {
                Ok((id, pipeline)) => {
                    index.insert(id, pipeline);
                }
                Err(err) => {
                    errors.push(err);
                }
            });
        if errors.is_empty() {
            Ok(index)
        } else {
            Err(errors)
        }
    }

    pub fn into_scoped_transforms(self) -> Vec<(ComponentKey, PipelineTransform)> {
        self.0
            .into_iter()
            .map(|(pipeline_id, pipeline)| pipeline.into_scoped_transforms(&pipeline_id))
            .flatten()
            .collect()
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct PipelineTransform {
    #[serde(flatten)]
    pub inner: TransformOuter,
    #[serde(default)]
    pub outputs: Vec<ComponentKey>,
}

impl PipelineTransform {
    pub fn into_scoped_inputs(self, pipeline_id: &str, local_ids: &HashSet<String>) -> Self {
        let inputs = self
            .inner
            .inputs
            .into_iter()
            .map(|component_id| {
                if local_ids.contains(component_id.id()) {
                    component_id.into_pipeline(pipeline_id)
                } else {
                    component_id
                }
            })
            .collect();
        Self {
            inner: TransformOuter {
                inputs,
                inner: self.inner.inner,
            },
            outputs: self.outputs,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Pipeline {
    #[serde(default)]
    pub transforms: IndexMap<ComponentKey, PipelineTransform>,
}

impl Pipeline {
    fn into_scoped_transforms(self, pipeline_id: &str) -> Vec<(ComponentKey, PipelineTransform)> {
        let transform_keys: HashSet<_> = self
            .transforms
            .keys()
            .map(|item| item.id().to_string())
            .collect();
        self.transforms
            .into_iter()
            .map(|(transform_id, transform)| {
                let transform_id = transform_id.into_pipeline(pipeline_id);
                let transform = transform.into_scoped_inputs(pipeline_id, &transform_keys);
                (transform_id, transform)
            })
            .collect()
    }

    pub fn load_from_file(file: &Path, format: Format) -> Result<(String, Self), String> {
        let filename = file
            .file_stem()
            .and_then(|name| name.to_str().map(ToString::to_string))
            .ok_or_else(|| format!("Could not read filename: {:?}", file))?;
        let content = fs::read_to_string(file)
            .map_err(|err| format!("Could not read content: {:?}, {}", file, err))?;
        deserialize(&content, Some(format))
            .map(|value| (filename, value))
            .map_err(|err| format!("Could not parse content: {:?}, {:?}", file, err))
    }

    #[cfg(test)]
    pub fn from_toml(input: &str) -> Self {
        deserialize(input, Some(Format::Toml)).unwrap()
    }

    #[cfg(test)]
    pub fn from_json(input: &str) -> Self {
        deserialize(input, Some(Format::Json)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        let src = r#"
        [transforms.first]
        inputs = ["input"]
        outputs = ["output"]
        type = "remap"
        source = ""
        "#;
        let result: Pipeline = deserialize(src, Some(Format::Toml)).unwrap();
        assert_eq!(result.transforms.len(), 1);
    }
}
