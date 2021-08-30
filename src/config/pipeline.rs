use super::{
    format::{deserialize, Format},
    ComponentId, TransformOuter,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub type Pipelines = IndexMap<String, Pipeline>;

#[derive(Deserialize, Serialize, Debug)]
pub struct PipelineTransform {
    #[serde(flatten)]
    pub inner: TransformOuter,
    #[serde(default)]
    pub outputs: Vec<ComponentId>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Pipeline {
    #[serde(default)]
    pub transforms: IndexMap<ComponentId, PipelineTransform>,
}

impl Pipeline {
    pub fn load_from_folder(folder: &Path) -> Result<Pipelines, Vec<String>> {
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
                    if path.is_file() {
                        Some(Self::load_from_file(&path))
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

    pub fn load_from_file(file: &Path) -> Result<(String, Self), String> {
        let format =
            Format::from_path(file).map_err(|err| format!("Could not read format: {:?}", err))?;
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
