use std::collections::HashMap;

#[derive(Deserialize)]
pub struct Config {
    pub sources: HashMap<String, Source>,
    pub sinks: HashMap<String, SinkOuter>,
    #[serde(default)]
    pub transforms: HashMap<String, TransformOuter>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Source {
    Splunk {
        // TODO: this should only be port
        address: std::net::SocketAddr,
    },
}

#[derive(Deserialize)]
pub struct SinkOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Sink,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Sink {
    Splunk { address: std::net::SocketAddr },
    Elasticsearch,
}

#[derive(Deserialize)]
pub struct TransformOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Transform,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Transform {
    Sampler { rate: u8, pass_list: Vec<String> },
    RegexParser { regex: String },
    FieldFilter { field: String, value: String },
}

// Helper methods for programming contstruction during tests
impl Config {
    pub fn empty() -> Self {
        Self {
            sources: HashMap::new(),
            sinks: HashMap::new(),
            transforms: HashMap::new(),
        }
    }

    pub fn add_source(&mut self, name: &str, source: Source) {
        self.sources.insert(name.to_string(), source);
    }

    pub fn add_sink(&mut self, name: &str, inputs: &[&str], sink: Sink) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let sink = SinkOuter {
            inner: sink,
            inputs,
        };

        self.sinks.insert(name.to_string(), sink);
    }

    pub fn add_transform(&mut self, name: &str, inputs: &[&str], transform: Transform) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let transform = TransformOuter {
            inner: transform,
            inputs,
        };

        self.transforms.insert(name.to_string(), transform);
    }
}
