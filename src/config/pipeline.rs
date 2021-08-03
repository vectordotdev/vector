use super::format::{deserialize, Format};
use super::TransformOuter;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PipelineTransform {
    #[serde(flatten)]
    pub inner: TransformOuter,
    pub outputs: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Pipeline {
    pub transforms: IndexMap<String, PipelineTransform>,
}

impl Pipeline {
    pub fn load_from_folder(folder: &PathBuf) -> Result<IndexMap<String, Self>, Vec<String>> {
        let entries = fs::read_dir(folder)
            .map_err(|err| {
                vec![format!(
                    "Could not list folder content: {:?}, {}",
                    folder, err
                )]
            })?
            .map(|entry| entry.unwrap().path())
            .filter(|entry| entry.is_file())
            .map(|entry| Self::load_from_file(&entry));
        let mut index = IndexMap::new();
        let mut errors = Vec::new();
        for entry in entries {
            match entry {
                Ok((id, pipeline)) => {
                    index.insert(id, pipeline);
                }
                Err(err) => {
                    errors.push(err);
                }
            };
        }
        if errors.is_empty() {
            Ok(index)
        } else {
            Err(errors)
        }
    }

    pub fn load_from_file(file: &PathBuf) -> Result<(String, Self), String> {
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
