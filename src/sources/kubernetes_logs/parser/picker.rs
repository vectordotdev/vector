use super::{cri::Cri, docker::Docker};
use crate::{
    event::{Event, Value},
    transforms::FunctionTransform,
};
use std::cell::RefCell;

#[derive(Clone, Debug)]
pub struct Picker {
    choice: RefCell<PickerChoice>,
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub enum PickerChoice {
    #[derivative(Default)]
    Init,
    Docker(Docker),
    Cri(Cri),
}

impl Picker {
    pub fn new() -> Self {
        Self { choice: RefCell::new(PickerChoice::Init), }
    }
}

impl FunctionTransform for Picker {
    fn transform(&self, output: &mut Vec<Event>, event: Event) {
        let mut choice = self.choice.borrow_mut();
        match &*choice {
            PickerChoice::Init => {
                let message = event
                    .as_log()
                    .get(crate::config::log_schema().message_key())
                    .expect("message key must be present");
                let bytes = if let Value::Bytes(bytes) = message {
                    bytes
                } else {
                    panic!("Message value must be in Bytes");
                };
                if bytes.len() > 1 && bytes[0] == b'{' {
                    *choice = PickerChoice::Docker(Docker);
                } else {
                    *choice = PickerChoice::Cri(Cri::new());
                }
                self.transform(output, event)
            }
            PickerChoice::Docker(t)  => t.transform(output, event),
            PickerChoice::Cri(t)  => t.transform(output, event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{cri, docker, test_util};
    use super::*;
    use crate::{event::LogEvent, transforms::Transform, Event};

    /// Picker has to work for all test cases for underlying parsers.
    fn cases() -> Vec<(String, LogEvent)> {
        let mut cases = vec![];
        cases.extend(docker::tests::cases());
        cases.extend(cri::tests::cases());
        cases
    }

    #[test]
    fn test_parsing() {
        test_util::test_parser(|| Transform::function(Picker::new()), cases());
    }

    #[test]
    fn test_parsing_invalid() {
        let cases = vec!["", "qwe", "{"];

        for message in cases {
            let input = Event::from(message);
            let picker = Picker::new();
            let output = picker.transform_one(input);
            assert!(output.is_none(), "Expected none: {:?}", output);
        }
    }
}
