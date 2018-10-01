use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::fs::{File, OpenOptions};

use serde;
use serde_json;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    message: String,
}

impl Record {
    pub fn new(msg: &str) -> Record {
        Record { message: msg.to_string() }
    }
}


#[derive(Clone)]
struct LogPosition {
    filename: String,
    offset: u64,
}

pub struct Producer {
    file: File,
    position: LogPosition,
}

impl Producer {
    pub fn new(filename: &str) -> io::Result<Producer> {
        let filename = filename.to_string();
        let file = OpenOptions::new().append(true).create(true).open(&filename)?;
        let offset = file.metadata()?.len();
        Ok(Producer { file, position: LogPosition { filename, offset } })
    }

    pub fn send(&mut self, records: &[Record]) -> io::Result<()> {
        for record in records {
            let encoded = serde_json::to_string(&record).expect("json encoding failure");
            let len = encoded.len();
            self.file.write_u32::<BigEndian>(len as u32)?;
            self.file.write_all(encoded.as_bytes())?;
            self.position.offset += 4 + len as u64;
        }
        Ok(())
    }

    pub fn build_consumer(&self) -> io::Result<Consumer> {
        Consumer::new(self.position.clone())
    }
}

pub struct Consumer {
    file: File,
    position: LogPosition,
}

impl Consumer {
    fn new(position: LogPosition) -> io::Result<Consumer> {
        let mut file = OpenOptions::new().read(true).open(&position.filename)?;
        let _pos = file.seek(SeekFrom::Start(position.offset))?;
        Ok(Consumer { file, position })
    }

    pub fn poll(&mut self) -> io::Result<Vec<Record>> {
        let mut records = Vec::new();
        loop {
            match self.file.read_u32::<BigEndian>() {
                Ok(len) => {
                    let mut de = serde_json::Deserializer::from_reader(&mut self.file);
                    let record: Record = serde::Deserialize::deserialize(&mut de).expect("failed to deserialize json");
                    records.push(record);
                    self.position.offset += 4 + len as u64;
                },
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    break
                },
                Err(e) => {
                    return Err(e)
                },
            }
        }
        Ok(records)
    }
}
