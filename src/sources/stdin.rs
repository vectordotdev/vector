use crate::{event::Event, topology::config::SourceConfig};
use codec::BytesDelimitedCodec;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use tokio::{
    codec::FramedRead,
    io::{stdin, AsyncRead},
};

#[derive(Deserialize, Serialize, Debug, Clone)]
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
    fn build(&self, out: mpsc::Sender<Event>) -> Result<super::Source, String> {
        Ok(stdin_source(stdin(), self.clone(), out))
    }
}

pub fn stdin_source<S>(stream: S, config: StdinConfig, out: mpsc::Sender<Event>) -> super::Source
where
    S: AsyncRead + Send + 'static,
{
    Box::new(future::lazy(move || {
        info!("Capturing STDIN");

        let source = FramedRead::new(
            stream,
            BytesDelimitedCodec::new_with_max_length(b'\n', config.max_length),
        )
        .map(Event::from)
        .map_err(|e| error!("error reading line: {:?}", e))
        .forward(out.sink_map_err(|e| error!("Error sending in sink {}", e)))
        .map(|_| info!("finished sending"));

        source
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event;
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
        let source = stdin_source(buf, config, tx);

        rt.block_on(source).unwrap();

        let event = rx.poll().unwrap();

        assert!(event.is_ready());
        assert_eq!(
            Ready(Some("hello world".into())),
            event.map(|event| event.map(|event| event.as_log()[&event::MESSAGE].to_string_lossy()))
        );

        let event = rx.poll().unwrap();
        assert!(event.is_ready());
        assert_eq!(
            Ready(Some("hello world again".into())),
            event.map(|event| event.map(|event| event.as_log()[&event::MESSAGE].to_string_lossy()))
        );

        let event = rx.poll().unwrap();
        assert!(event.is_ready());
        assert_eq!(Ready(None), event);
    }
}
