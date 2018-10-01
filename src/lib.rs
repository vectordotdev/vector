extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate byteorder;

use std::io;
use std::io::prelude::*;
use std::fs::{File, OpenOptions};
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

pub struct Producer {
    file: File,
}

impl Producer {
    pub fn new(filename: &str) -> io::Result<Producer> {
        OpenOptions::new().append(true).create(true).open(filename)
            .map(|file| Producer { file })
    }

    pub fn send(&mut self, records: &[Record]) -> io::Result<()> {
        for record in records {
            let encoded = serde_json::to_string(&record).expect("json encoding failure");
            let len = encoded.len() as u32;
            self.file.write_u32::<BigEndian>(len)?;
            self.file.write_all(encoded.as_bytes())?;
        }
        Ok(())
    }
}

pub struct Consumer {
    file: File,
}

impl Consumer {
    pub fn new(filename: &str) -> io::Result<Consumer> {
        OpenOptions::new().read(true).open(filename)
            .map(|file| Consumer { file })
    }

    pub fn poll(&mut self) -> io::Result<Vec<Record>> {
        let mut records = Vec::new();
        loop {
            match self.file.read_u32::<BigEndian>() {
                Ok(len) => {
                    let mut de = serde_json::Deserializer::from_reader(&mut self.file);
                    let record: Record = serde::Deserialize::deserialize(&mut de).expect("failed to deserialize json");
                    records.push(record);
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

#[cfg(test)]
mod test {
    use std::fs::remove_file;
    use super::{Producer, Consumer, Record};

    #[test]
    fn basic_write_then_read() {
        let filename = "logs/foo.log";
        remove_file(&filename).expect("error truncating file");

        let mut producer = Producer::new(filename).expect("failed to build producer");
        let mut consumer = Consumer::new(filename).expect("failed to build consumer");

        let batch_in = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];

        producer.send(&batch_in).expect("failed to send batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_in, batch_out);
    }
}
