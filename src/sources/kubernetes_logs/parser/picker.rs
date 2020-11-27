use super::{cri::Cri, docker::Docker};
use crate::{
    event::{Event, Value},
    internal_events::KubernetesLogsFormatPickerEdgeCase,
    transforms::FunctionTransform,
};

#[derive(Clone, Debug)]
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

impl FunctionTransform for Picker {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        match self {
            Picker::Init => {
                let message = match event
                    .as_log()
                    .get(crate::config::log_schema().message_key())
                {
                    Some(message) => message,
                    None => {
                        emit!(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event with no message field"
                        });
                        return;
                    }
                };

                let bytes = match message {
                    Value::Bytes(bytes) => bytes,
                    _ => {
                        emit!(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event with non-bytes message field"
                        });
                        return;
                    }
                };

                if bytes.len() > 1 && bytes[0] == b'{' {
                    *self = Picker::Docker(Docker)
                } else {
                    *self = Picker::Cri(Cri::new())
                }
                self.transform(output, event)
            }
            Picker::Docker(t) => t.transform(output, event),
            Picker::Cri(t) => t.transform(output, event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{cri, docker, test_util};
    use super::*;
    use crate::{event::LogEvent, test_util::trace_init, transforms::Transform, Event};

    /// Picker has to work for all test cases for underlying parsers.
    fn cases() -> Vec<(String, Vec<LogEvent>)> {
        let mut cases = vec![];
        cases.extend(docker::tests::cases());
        cases.extend(cri::tests::cases());
        cases
    }

    #[test]
    fn test_parsing() {
        trace_init();
        test_util::test_parser(|| Transform::function(Picker::new()), cases());
    }

    #[test]
    fn test_parsing_invalid() {
        trace_init();

        let cases = vec!["", "qwe", "{"];

        for message in cases {
            let input = Event::from(message);
            let mut picker = Picker::new();
            let mut output = Vec::new();
            picker.transform(&mut output, input);
            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }

    #[test]
    fn test_parsing_invalid_non_standard_events() {
        trace_init();

        let cases = vec![
            // No `message` field.
            Event::new_empty_log(),
            // Non-bytes `message` field.
            {
                let mut input = Event::new_empty_log();
                input.as_mut_log().insert("message", 123);
                input
            },
        ];

        for input in cases {
            let mut picker = Picker::new();
            let mut output = Vec::new();
            picker.transform(&mut output, input);
            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }
}
