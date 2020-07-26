use super::{cri::Cri, docker::Docker};
use crate::{
    event::{self, Event, Value},
    transforms::Transform,
};

pub enum Picker {
    Init,
    Docker(Docker),
    Cri(Cri),
}

impl Picker {
    pub fn new() -> Self {
        Picker::Init
    }
}

impl Transform for Picker {
    fn transform(&mut self, event: Event) -> Option<Event> {
        match self {
            Picker::Init => {
                let message = event
                    .as_log()
                    .get(event::log_schema().message_key())
                    .expect("message key must be present");
                let bytes = if let Value::Bytes(bytes) = message {
                    bytes
                } else {
                    panic!("message value must be Bytes");
                };
                if bytes.len() > 1 && bytes[0] == b'{' {
                    *self = Picker::Docker(Docker)
                } else {
                    *self = Picker::Cri(Cri::new())
                }
                self.transform(event)
            }
            Picker::Docker(t) => t.transform(event),
            Picker::Cri(t) => t.transform(event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{cri, docker, test_util};
    use super::{Picker, Transform};
    use crate::{event::LogEvent, Event};

    /// Picker has to work for all test cases for underlying parsers.
    fn cases() -> Vec<(String, LogEvent)> {
        let mut cases = vec![];
        cases.extend(docker::tests::cases());
        cases.extend(cri::tests::cases());
        cases
    }

    #[test]
    fn test_parsing() {
        test_util::test_parser(Picker::new, cases());
    }

    #[test]
    fn test_parsing_invalid() {
        let cases = vec!["", "qwe", "{"];

        for message in cases {
            let input = Event::from(message);
            let mut picker = Picker::new();
            let output = picker.transform(input);
            assert!(output.is_none());
        }
    }
}
