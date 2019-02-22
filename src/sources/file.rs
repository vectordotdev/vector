use crate::record::Record;
use futures::{future, sync::mpsc, Future, Sink};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct FileConfig {
    pub include: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
    pub context_key: Option<String>,
    pub start_at_beginning: bool,
    pub ignore_older: Option<u64>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            include: vec![],
            exclude: vec![],
            context_key: Some("file".to_string()),
            start_at_beginning: false,
            ignore_older: None,
        }
    }
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

    let ignore_before = config
        .ignore_older
        .map(|secs| SystemTime::now() - Duration::from_secs(secs));

    let cernan_server = cernan_file_source::file_server::FileServer {
        include: config.include.clone(),
        exclude: config.exclude.clone(),
        max_read_bytes: 2048,
        start_at_beginning: config.start_at_beginning,
        ignore_before,
    };

    let context_key = config.context_key.clone().map(Atom::from);

    let out = out.sink_map_err(|_| ()).with(move |(line, file)| {
        let mut record = Record::new_from_line(line);
        if let Some(ref context_key) = context_key {
            record.custom.insert(context_key.clone(), file);
        }
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
