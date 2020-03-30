use super::Transform;
use crate::{
    event::{self, Value},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    Event,
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AnsiStripperConfig {
    field: Option<Atom>,
}

inventory::submit! {
    TransformDescription::new_without_default::<AnsiStripperConfig>("ansi_stripper")
}

#[typetag::serde(name = "ansi_stripper")]
impl TransformConfig for AnsiStripperConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());

        Ok(Box::new(AnsiStripper {
            field: field.clone(),
        }))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "ansi_stripper"
    }
}

pub struct AnsiStripper {
    field: Atom,
}

impl Transform for AnsiStripper {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();

        match log.get_mut(&self.field) {
            None => debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
            ),
            Some(Value::Bytes(ref mut bytes)) => {
                *bytes = match strip_ansi_escapes::strip(bytes.clone()) {
                    Ok(b) => b.into(),
                    Err(error) => {
                        debug!(
                            message = "Could not strip ANSI escape sequences.",
                            field = self.field.as_ref(),
                            %error,
                            rate_limit_secs = 30,
                        );
                        return Some(event);
                    }
                };
            }
            _ => debug!(
                message = "Field value must be a string.",
                field = self.field.as_ref(),
            ),
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::AnsiStripper;
    use crate::{
        event::{self, Event, Value},
        transforms::Transform,
    };

    macro_rules! assert_foo_bar {
        ($($in:expr),* $(,)?) => {
            $(
                let mut transform = AnsiStripper {
                    field: "message".into(),
                };

                let event = Event::from($in);
                let event = transform.transform(event).unwrap();

                assert_eq!(
                    event.into_log().remove(&event::log_schema().message_key()).unwrap(),
                    Value::from("foo bar")
                );
            )+
        };
    }

    #[test]
    fn ansi_stripper_transform() {
        assert_foo_bar![
            "\x1b[3;4Hfoo bar",
            "\x1b[3;4ffoo bar",
            "\x1b[3Afoo bar",
            "\x1b[3Bfoo bar",
            "\x1b[3Cfoo bar",
            "\x1b[3Dfoo bar",
            "\x1b[sfoo bar",
            "\x1b[ufoo bar",
            "\x1b[2Jfoo bar",
            "\x1b[Kfoo bar",
            "\x1b[32mfoo\x1b[m bar",
            "\x1b[46mfoo\x1b[0m bar",
            "foo \x1b[46;41mbar",
            "\x1b[=3hfoo bar",
            "\x1b[=3lfoo bar",
            "foo bar",
        ];
    }
}
