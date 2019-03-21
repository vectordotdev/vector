use crate::topology::config::SourceConfig;
use futures::sync::mpsc;
use tokio::{codec::LinesCodec, io::stdin};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {}

#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        Ok(stdin_source(self, out))
    }
}

pub fn stdin_source(config: &StdinConfig, out: mpsc::Sender<Record>) -> super::Source {
    LinesCodec::new().framed(stdin()).forward(out)
}
