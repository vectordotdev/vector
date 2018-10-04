#[macro_use]
extern crate log;

extern crate byteorder;
extern crate uuid;

#[cfg(test)]
extern crate tempdir;

pub mod transport;

#[cfg(test)]
mod test {
    use super::transport::{Consumer, Coordinator};
    use tempdir::TempDir;

    static MESSAGES: &[&[u8]] = &[
        b"i am the first message",
        b"i am the second message",
        b"i am the third message",
        b"i am the fourth message",
    ];

    #[test]
    fn basic_write_then_read() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");
        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        log.append(MESSAGES).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_out, MESSAGES);
    }

    #[test]
    fn consumer_starts_from_the_end() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");

        log.append(&MESSAGES[0..2]).expect("failed to append batch");

        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        log.append(&MESSAGES[2..4]).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_out, &MESSAGES[2..4]);
    }

    #[test]
    fn logs_split_into_segments() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");
        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        log.append(&MESSAGES[..1])
            .expect("failed to append first record");

        // make this auto with config?
        log.roll_segment().expect("failed to roll new segment");

        log.append(&MESSAGES[1..]).expect("failed to append batch");

        assert_eq!(2, ::std::fs::read_dir(&dir).unwrap().count());
        assert_eq!(consumer.poll().expect("failed to poll"), MESSAGES);
    }

    #[test]
    fn only_retains_segments_with_active_consumers() {
        let dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::default();
        let mut log = coordinator.create_log(&dir).expect("failed to build log");
        let mut consumer = Consumer::new(&dir).expect("failed to build consumer");

        log.append(&MESSAGES[..1])
            .expect("failed to append first record");

        // make this auto with config
        log.roll_segment().expect("failed to roll new segment");

        log.append(&MESSAGES[1..]).expect("failed to append batch");

        assert_eq!(2, ::std::fs::read_dir(&dir).unwrap().count());
        assert_eq!(consumer.poll().expect("failed to poll"), MESSAGES);
        consumer.commit_offsets(&mut coordinator);

        // make this auto
        coordinator
            .enforce_retention()
            .expect("failed to enforce retention");
        assert_eq!(1, ::std::fs::read_dir(&dir).unwrap().count());
    }
}
