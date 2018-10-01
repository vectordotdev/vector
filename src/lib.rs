extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate byteorder;

use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
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
    filename: String,
    file: File,
    offset: u64,
}

impl Producer {
    pub fn new(filename: &str) -> io::Result<Producer> {
        let filename = filename.to_string();
        let file = OpenOptions::new().append(true).create(true).open(&filename)?;
        let offset = file.metadata()?.len();
        Ok(Producer { file, filename, offset })
    }

    pub fn send(&mut self, records: &[Record]) -> io::Result<()> {
        for record in records {
            let encoded = serde_json::to_string(&record).expect("json encoding failure");
            let len = encoded.len();
            self.file.write_u32::<BigEndian>(len as u32)?;
            self.file.write_all(encoded.as_bytes())?;
            self.offset += 4 + len as u64;
        }
        Ok(())
    }

    pub fn build_consumer(&self) -> io::Result<Consumer> {
        Consumer::new(&self.filename, self.offset)
    }
}

pub struct Consumer {
    file: File,
}

impl Consumer {
    fn new(filename: &str, offset: u64) -> io::Result<Consumer> {
        let mut file = OpenOptions::new().read(true).open(filename)?;
        let _pos = file.seek(SeekFrom::Start(offset))?;
        Ok(Consumer { file })
    }

    pub fn poll(&mut self) -> io::Result<Vec<Record>> {
        let mut records = Vec::new();
        loop {
            match self.file.read_u32::<BigEndian>() {
                Ok(_len) => {
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
    use super::{Producer, Record};

    #[test]
    fn basic_write_then_read() {
        let filename = "logs/foo.log";
        remove_file(&filename).expect("error truncating file");

        let mut producer = Producer::new(filename).expect("failed to build producer");
        let mut consumer = producer.build_consumer().expect("failed to build consumer");

        let batch_in = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];

        producer.send(&batch_in).expect("failed to send batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_in, batch_out);
    }

    #[test]
    fn consumer_starts_from_the_end() {
        let filename = "logs/bar.log";
        remove_file(&filename).expect("error truncating file");

        let mut producer = Producer::new(filename).expect("failed to build producer");

        let first_batch = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];
        producer.send(&first_batch).expect("failed to send batch");

        let mut consumer = producer.build_consumer().expect("failed to build consumer");

        let second_batch = vec![
            Record::new("i am the third message"),
            Record::new("i am the fourth message"),
        ];
        producer.send(&second_batch).expect("failed to send batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(second_batch, batch_out);
    }
}
