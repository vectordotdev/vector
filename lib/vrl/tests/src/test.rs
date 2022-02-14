use std::{collections::BTreeMap, fs, path::Path};

use vrl::{function::Example, Value};

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
        }
    }

    pub fn from_example(func: &'static str, example: &Example) -> Self {
        let object = Value::Object(BTreeMap::default());
        let result = match example.result {
            Ok(string) => string.to_owned(),
            Err(err) => err.to_string(),
        };

        Self {
            name: example.title.to_owned(),
            category: format!("functions/{}", func),
            error: None,
            source: example.source.to_owned(),
            object,
            result,
            result_approx: false,
            skip: false,
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
        .replace("_", " ")
}
