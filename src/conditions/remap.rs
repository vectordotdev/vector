use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription},
    Event,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct RemapConfig {
    source: String,
}

inventory::submit! {
    ConditionDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[typetag::serde(name = "remap")]
impl ConditionConfig for RemapConfig {
    fn build(&self) -> crate::Result<Box<dyn Condition>> {
        // TODO(jean): move this to into a global "immutable functions" array.
        use crate::remap::*;
        let functions: Vec<Box<dyn remap::Function>> = vec![
            Box::new(Split),
            Box::new(ToString),
            Box::new(ToInt),
            Box::new(ToFloat),
            Box::new(ToBool),
            Box::new(ToTimestamp),
            Box::new(Upcase),
            Box::new(Downcase),
            Box::new(UuidV4),
            Box::new(Sha1),
            Box::new(Md5),
            Box::new(Now),
            Box::new(FormatTimestamp),
            Box::new(Contains),
            Box::new(StartsWith),
            Box::new(EndsWith),
            Box::new(Slice),
            Box::new(Tokenize),
            Box::new(Sha2),
            Box::new(Sha3),
            Box::new(ParseDuration),
            Box::new(FormatNumber),
            Box::new(ParseUrl),
            Box::new(Ceil),
            Box::new(Floor),
            Box::new(Round),
            Box::new(ParseSyslog),
            Box::new(ParseTimestamp),
            Box::new(ParseJson),
            Box::new(Truncate),
            Box::new(StripWhitespace),
            Box::new(StripAnsiEscapeCodes),
        ];

        let program = remap::Program::new(&self.source, functions)?;

        Ok(Box::new(Remap { program }))
    }
}

//------------------------------------------------------------------------------

pub struct Remap {
    program: remap::Program,
}

impl Remap {
    fn execute(&self, event: &Event) -> remap::Result<Option<remap::Value>> {
        // TODO(jean): This clone exists until remap-lang has an "immutable"
        // mode.
        //
        // For now, mutability in reduce "remap ends-when conditions" is
        // allowed, but it won't mutate the original event, since we cloned it
        // here.
        //
        // Having first-class immutability support in the language allows for
        // more performance (one less clone), and boot-time errors when a
        // program wants to mutate its events.
        remap::Runtime::default().execute(&mut event.clone(), &self.program)
    }
}

impl Condition for Remap {
    fn check(&self, event: &Event) -> bool {
        self.execute(&event)
            .ok()
            .flatten()
            .map(|value| match value {
                remap::Value::Boolean(boolean) => boolean,
                _ => false,
            })
            .unwrap_or(false)
    }

    fn check_with_context(&self, event: &Event) -> Result<(), String> {
        self.execute(event)
            .map_err(|err| format!("source execution failed: {}", err))?
            .ok_or("source execution resolved to no value".into())
            .and_then(|value| match value {
                remap::Value::Boolean(v) if v => Ok(()),
                remap::Value::Boolean(v) if !v => Err("source execution resolved to false".into()),
                _ => Err("source execution resolved to non-boolean value".into()),
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::log_event;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemapConfig>();
    }

    #[test]
    fn check_remap() {
        let checks = vec![
            (
                log_event![],   // event
                "true == true", // source
                Ok(()),         // build result
                Ok(()),         // check result
            ),
            (
                log_event!["foo" => true, "bar" => false],
                ".bar || .foo",
                Ok(()),
                Ok(()),
            ),
            (
                log_event![],
                "true == false",
                Ok(()),
                Err("source execution resolved to false"),
            ),
            (
                log_event![],
                "",
                Ok(()),
                Err("source execution resolved to no value"),
            ),
            (
                log_event!["foo" => "string"],
                ".foo",
                Ok(()),
                Err("source execution resolved to non-boolean value"),
            ),
            (
                log_event![],
                ".",
                Err(
                    "parser error:  --> 1:2\n  |\n1 | .\n  |  ^---\n  |\n  = expected path_segment",
                ),
                Ok(()),
            ),
        ];

        for (event, source, build, check) in checks {
            let source = source.to_owned();
            let config = RemapConfig { source };

            assert_eq!(
                config.build().map(|_| ()).map_err(|e| e.to_string()),
                build.map_err(|e| e.to_string())
            );

            if let Ok(cond) = config.build() {
                assert_eq!(
                    cond.check_with_context(&event),
                    check.map_err(|e| e.to_string())
                );
            }
        }
    }
}
