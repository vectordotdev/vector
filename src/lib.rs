extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate byteorder;

#[cfg(test)]
extern crate tempdir;

pub mod log;

#[cfg(test)]
mod test {
    use tempdir::TempDir;
    use super::log::{Producer, Record};

    #[test]
    fn basic_write_then_read() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");
        let path = dir.path().join("read_write.log");

        let mut producer = Producer::new(path).expect("failed to build producer");
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
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");
        let path = dir.path().join("from_end.log");

        let mut producer = Producer::new(path).expect("failed to build producer");

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
