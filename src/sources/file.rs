use crate::record::Record;
use futures::{future, sync::mpsc, Future, Sink};
use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;
use std::thread;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub path: PathBuf,
}

#[typetag::serde(name = "file")]
impl crate::topology::config::SourceConfig for FileConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        // TODO: validate paths
        Ok(file_source(self, out))
    }
}

pub fn file_source(config: &FileConfig, out: mpsc::Sender<Record>) -> super::Source {
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    let cernan_server = cernan_file_source::file_server::FileServer {
        path: config.path.clone(),
        max_read_bytes: 2048,
    };

    let out = out.sink_map_err(|_| ()).with(|(line, file)| {
        let mut record = Record::new_from_line(line);
        record.custom.insert("file".into(), file);
        future::ok(record)
    });

    Box::new(future::lazy(|| {
        thread::spawn(move || {
            cernan_server.run(out, shutdown_rx);
        });

        // Dropping shutdown_tx is how we signal to the file server that it's time to shut down,
        // so it needs to be held onto until the future we return is dropped.
        future::empty().inspect(|_| drop(shutdown_tx))
    }))
}
