use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Value,
    internal_events::{
        ANSIStripperEventProcessed, ANSIStripperFailed, ANSIStripperFieldInvalid,
        ANSIStripperFieldMissing,
    },
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
    TransformDescription::new::<AnsiStripperConfig>("ansi_stripper")
}

impl GenerateConfig for AnsiStripperConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "ansi_stripper")]
impl TransformConfig for AnsiStripperConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .clone()
            .unwrap_or_else(|| Atom::from(crate::config::log_schema().message_key()));

        Ok(Box::new(AnsiStripper { field }))
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
            None => emit!(ANSIStripperFieldMissing { field: &self.field }),
            Some(Value::Bytes(ref mut bytes)) => {
                match strip_ansi_escapes::strip(&bytes) {
                    Ok(b) => *bytes = b.into(),
                    Err(error) => emit!(ANSIStripperFailed {
                        field: &self.field,
                        error
                    }),
                };
            }
            _ => emit!(ANSIStripperFieldInvalid { field: &self.field }),
        }

        emit!(ANSIStripperEventProcessed);

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::AnsiStripper;
    use crate::{
        event::{Event, Value},
        transforms::Transform,
    };
    use string_cache::DefaultAtom as Atom;

    macro_rules! assert_foo_bar {
        ($($in:expr),* $(,)?) => {
            $(
                let mut transform = AnsiStripper {
                    field: "message".into(),
                };

                let event = Event::from($in);
                let event = transform.transform(event).unwrap();

                assert_eq!(
                    event.into_log().remove(&Atom::from(crate::config::log_schema().message_key())).unwrap(),
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
