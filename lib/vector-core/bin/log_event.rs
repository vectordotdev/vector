use vector_core::event::LogEvent;

fn main() {
    let log = LogEvent::default();
    let query = "a";

    for _ in 0..u16::MAX {
        log.contains(query);
    }
}
