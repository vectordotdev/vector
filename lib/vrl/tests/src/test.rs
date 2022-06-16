use std::str::FromStr;
use std::{collections::BTreeMap, fs, path::Path};

use ::value::Value;
use lookup::LookupBuf;
use vrl::function::Example;

#[derive(Debug)]
pub struct Test {
    pub name: String,
    pub category: String,
    pub error: Option<String>,
    pub source: String,
    pub object: Value,
    pub result: String,
    pub result_approx: bool,
    pub skip: bool,

    // paths set to read-only. (can be merged once paths support metadata)
    pub read_only_paths: Vec<(LookupBuf, bool)>,
    pub read_only_metadata_paths: Vec<(LookupBuf, bool)>,
}

enum CaptureMode {
    Result,
    Object,
    None,
    Done,
}

impl Test {
    pub fn from_path(path: &Path) -> Self {
        let name = test_name(path);
        let category = test_category(path);
        let content = fs::read_to_string(path).expect("content");

        let mut source = String::new();
        let mut object = String::new();
        let mut result = String::new();
        let mut result_approx = false;
        let mut skip = false;

        if content.starts_with("# SKIP") {
            skip = true;
        }

        let mut read_only_paths = vec![];
        let mut read_only_metadata_paths = vec![];

        let mut capture_mode = CaptureMode::None;
        for mut line in content.lines() {
            if line.starts_with('#') && !matches!(capture_mode, CaptureMode::Done) {
                line = line.strip_prefix('#').expect("prefix");
                line = line.strip_prefix(' ').unwrap_or(line);

                if line.starts_with("object:") {
                    capture_mode = CaptureMode::Object;
                    line = line.strip_prefix("object:").expect("object").trim_start();
                } else if line.starts_with("result: ~") {
                    capture_mode = CaptureMode::Result;
                    result_approx = true;
                    line = line.strip_prefix("result: ~").expect("result").trim_start();
                } else if line.starts_with("result:") {
                    capture_mode = CaptureMode::Result;
                    line = line.strip_prefix("result:").expect("result").trim_start();
                } else if line.starts_with("read_only:") {
                    let path_str = line.strip_prefix("read_only:").expect("read-only");
                    read_only_paths.push((FromStr::from_str(path_str).expect("valid path"), false));
                    continue;
                } else if line.starts_with("read_only_recursive:") {
                    let path_str = line
                        .strip_prefix("read_only_recursive:")
                        .expect("read-only");
                    read_only_paths.push((FromStr::from_str(path_str).expect("valid path"), true));
                    continue;
                } else if line.starts_with("read_only_metadata:") {
                    let path_str = line
                        .strip_prefix("read_only_metadata:")
                        .expect("read_only_metadata");
                    read_only_metadata_paths
                        .push((FromStr::from_str(path_str).expect("valid path"), false));
                    continue;
                } else if line.starts_with("read_only_metadata_recursive:") {
                    let path_str = line
                        .strip_prefix("read_only_metadata_recursive:")
                        .expect("read-read_only_metadata_recursive");
                    read_only_metadata_paths
                        .push((FromStr::from_str(path_str).expect("valid path"), true));
                    continue;
                }

                match capture_mode {
                    CaptureMode::None | CaptureMode::Done => continue,
                    CaptureMode::Result => {
                        result.push_str(line);
                        result.push('\n');
                    }
                    CaptureMode::Object => {
                        object.push_str(line);
                    }
                }
            } else {
                capture_mode = CaptureMode::Done;

                source.push_str(line);
                source.push('\n')
            }
        }

        let mut error = None;
        let object = if object.is_empty() {
            Value::Object(BTreeMap::default())
        } else {
            match serde_json::from_str::<'_, Value>(&object) {
                Ok(value) => value,
                Err(err) => {
                    error = Some(format!("unable to parse object as JSON: {}", err));
                    Value::Null
                }
            }
        };

        result = result.trim_end().to_owned();

        Self {
            name,
            category,
            error,
            source,
            object,
            result,
            result_approx,
            skip,
            read_only_paths,
            read_only_metadata_paths,
        }
    }

    pub fn from_example(func: impl ToString, example: &Example) -> Self {
        let object = Value::Object(BTreeMap::default());
        let result = match example.result {
            Ok(string) => string.to_owned(),
            Err(err) => err.to_string(),
        };

        Self {
            name: example.title.to_owned(),
            category: format!("functions/{}", func.to_string()),
            error: None,
            source: example.source.to_owned(),
            object,
            result,
            result_approx: false,
            skip: false,
            read_only_paths: vec![],
            read_only_metadata_paths: vec![],
        }
    }
}

fn test_category(path: &Path) -> String {
    if path.as_os_str() == "tests/example.vrl" {
        return "".to_owned();
    }

    path.to_string_lossy()
        .strip_prefix("tests/")
        .expect("test")
        .rsplit_once('/')
        .map_or(
            path.to_string_lossy()
                .strip_prefix("tests/")
                .unwrap()
                .to_owned(),
            |x| x.0.to_owned(),
        )
}

fn test_name(path: &Path) -> String {
    path.to_string_lossy()
        .rsplit_once('/')
        .unwrap()
        .1
        .trim_end_matches(".vrl")
        .replace('_', " ")
}
