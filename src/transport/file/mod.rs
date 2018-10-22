use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod consumer;
mod coordinator;
mod log;

pub use self::{consumer::*, coordinator::*, log::*};

// if we put this in log we have to create one, which is side effect-y
fn get_segment_paths(dir: &Path) -> io::Result<impl Iterator<Item = PathBuf>> {
    fs::read_dir(dir)?
        .map(|r| r.map(|entry| entry.path()))
        .collect::<Result<Vec<PathBuf>, _>>()
        .map(|r| r.into_iter())
}

#[cfg(test)]
mod test {
    use super::{Consumer, Coordinator, Log};
    use tempdir::TempDir;

    static MESSAGES: &[&[u8]] = &[
        b"i am the first message",
        b"i am the second message",
        b"i am the third message",
        b"i am the fourth message",
    ];

    fn setup(topic: &str) -> (TempDir, Coordinator, Log, Consumer) {
        let data_dir = TempDir::new_in(".", "logs").expect("creating tempdir");

        let mut coordinator = Coordinator::new(&data_dir);
        let log = coordinator.create_log(topic).expect("failed to build log");
        let consumer = coordinator
            .build_consumer(topic)
            .expect("failed to build consumer");
        (data_dir, coordinator, log, consumer)
    }

    #[test]
    fn basic_write_then_read() {
        let (_data_dir, _coordinator, mut log, mut consumer) = setup("foo");

        log.append(MESSAGES).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_out, MESSAGES);
    }

    #[test]
    fn consumer_starts_from_the_end() {
        let (_data_dir, coordinator, mut log, _) = setup("foo");

        log.append(&MESSAGES[0..2]).expect("failed to append batch");

        let mut consumer = coordinator
            .build_consumer("foo")
            .expect("failed to build consumer");

        log.append(&MESSAGES[2..4]).expect("failed to append batch");

        let batch_out = consumer.poll().expect("failed to poll for batch");
        assert_eq!(batch_out, &MESSAGES[2..4]);
    }

    #[test]
    fn logs_split_into_segments() {
        let (_data_dir, _coordinator, mut log, mut consumer) = setup("foo");

        log.append(&MESSAGES[..1])
            .expect("failed to append first record");

        // make this auto with config?
        log.roll_segment().expect("failed to roll new segment");

        log.append(&MESSAGES[1..]).expect("failed to append batch");

        assert_eq!(2, log.get_segments().unwrap().count());
        assert_eq!(consumer.poll().expect("failed to poll"), MESSAGES);
    }

    #[test]
    fn only_retains_segments_with_active_consumers() {
        let (_data_dir, mut coordinator, mut log, mut consumer) = setup("foo");

        log.append(&MESSAGES[..1])
            .expect("failed to append first record");

        // make this auto with config
        log.roll_segment().expect("failed to roll new segment");

        log.append(&MESSAGES[1..]).expect("failed to append batch");

        assert_eq!(2, log.get_segments().unwrap().count());
        assert_eq!(consumer.poll().expect("failed to poll"), MESSAGES);
        consumer.commit_offsets(&mut coordinator);

        // make this auto
        coordinator
            .enforce_retention()
            .expect("failed to enforce retention");
        assert_eq!(1, log.get_segments().unwrap().count());
    }
}
