use crate::{record::Record, topology::config::SourceConfig};
use futures::{sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use tokio::{
    codec::{FramedRead, LinesCodec},
    io::{stdin, AsyncRead},
};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    #[serde(default = "default_max_length")]
    pub max_length: usize,
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: default_max_length(),
        }
    }
}

fn default_max_length() -> usize {
    100 * 1024
}

#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        Ok(stdin_source(stdin(), self, out))
    }
}

pub fn stdin_source<S>(stream: S, config: &StdinConfig, out: mpsc::Sender<Record>) -> super::Source
where
    S: AsyncRead + Send + 'static,
{
    info!("Capturing STDIN");

    let source = FramedRead::new(stream, LinesCodec::new_with_max_length(config.max_length))
        .map(Record::from)
        .map_err(|e| error!("error reading line: {:?}", e))
        .forward(out.sink_map_err(|e| error!("Error sending in sink {}", e)))
        .map(|_| info!("finished sending"));

    Box::new(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::sync::mpsc;
    use futures::Async::*;
    use std::io::Cursor;
    use tokio::runtime::current_thread::Runtime;

    #[test]
    fn stdin_decodes_line() {
        let (tx, mut rx) = mpsc::channel(10);
        let config = StdinConfig::default();
        let buf = Cursor::new(String::from("hello world\nhello world again"));

        let mut rt = Runtime::new().unwrap();
        let source = stdin_source(buf, &config, tx);

        rt.block_on(source).unwrap();

        let record = rx.poll().unwrap();

        assert!(record.is_ready());
        assert_eq!(
            Ready(Some("hello world".into())),
            record.map(|r| r.map(|r| r.to_string_lossy()))
        );

        let record = rx.poll().unwrap();
        assert!(record.is_ready());
        assert_eq!(
            Ready(Some("hello world again".into())),
            record.map(|r| r.map(|r| r.to_string_lossy()))
        );

        let record = rx.poll().unwrap();
        assert!(record.is_ready());
        assert_eq!(Ready(None), record);
    }
}
