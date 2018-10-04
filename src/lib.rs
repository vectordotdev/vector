#[macro_use]
extern crate log;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate byteorder;
extern crate uuid;

#[cfg(test)]
extern crate tempdir;

pub mod transport;

#[cfg(test)]
mod test {
    use tempdir::TempDir;
    use super::transport::{Coordinator, Consumer, Record};

    #[test]
    fn basic_write_then_read() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");
        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        let batch_in = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];

        log.append(&batch_in).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_in, batch_out);
    }

    #[test]
    fn consumer_starts_from_the_end() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");

        let first_batch = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];
        log.append(&first_batch).expect("failed to append batch");

        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        let second_batch = vec![
            Record::new("i am the third message"),
            Record::new("i am the fourth message"),
        ];
        log.append(&second_batch).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(second_batch, batch_out);
    }

    #[test]
    fn logs_split_into_segments() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");
        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        let records = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];
        log.append(&records[..1]).expect("failed to append first record");

        // make this auto with config?
        log.roll_segment().expect("failed to roll new segment");

        log.append(&records[1..]).expect("failed to append batch");

        assert_eq!(2, ::std::fs::read_dir(&dir).unwrap().count());
        assert_eq!(records, consumer.poll().expect("failed to poll"));
    }

    #[test]
    fn only_retains_segments_with_active_consumers() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");
        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        let records = vec![
            Record::new("i am the first message"),
            Record::new("i am the second message"),
        ];
        log.append(&records[..1]).expect("failed to append first record");

        // make this auto with config
        log.roll_segment().expect("failed to roll new segment");

        log.append(&records[1..]).expect("failed to append batch");

        assert_eq!(2, ::std::fs::read_dir(&dir).unwrap().count());
        assert_eq!(records, consumer.poll().expect("failed to poll"));
        consumer.commit_offsets(&mut coordinator);

        // make this auto
        coordinator.enforce_retention().expect("failed to enforce retention");
        assert_eq!(1, ::std::fs::read_dir(&dir).unwrap().count());
    }
}
