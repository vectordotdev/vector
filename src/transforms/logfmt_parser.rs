use super::Transform;
use crate::{
    event::{self, Event},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    types::{parse_conversion_map, Conversion},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct LogfmtConfig {
    pub field: Option<Atom>,
    pub drop_field: bool,
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<LogfmtConfig>("logfmt_parser")
}

#[typetag::serde(name = "logfmt_parser")]
impl TransformConfig for LogfmtConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());
        let conversions = parse_conversion_map(&self.types)?;

        Ok(Box::new(Logfmt {
            field: field.clone(),
            drop_field: self.drop_field,
            conversions,
        }))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "logfmt_parser"
    }
}

pub struct Logfmt {
    field: Atom,
    drop_field: bool,
    conversions: HashMap<Atom, Conversion>,
}

impl Transform for Logfmt {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.to_string_lossy());

        let mut drop_field = self.drop_field;
        if let Some(value) = &value {
            let pairs = logfmt::parse(value)
                .into_iter()
                // Filter out pairs with None value (i.e. non-logfmt data)
                .filter_map(|logfmt::Pair { key, val }| val.map(|val| (Atom::from(key), val)));

            for (key, val) in pairs {
                if key == self.field {
                    drop_field = false;
                }

                if let Some(conv) = self.conversions.get(&key) {
                    match conv.convert(val.as_bytes().into()) {
                        Ok(value) => {
                            event.as_mut_log().insert(key, value);
                        }
                        Err(error) => {
                            debug!(
                                message = "Could not convert types.",
                                key = &key[..],
                                %error,
                                rate_limit_secs = 30
                            );
                        }
                    }
                } else {
                    event.as_mut_log().insert(key, val);
                }
            }

            if drop_field {
                event.as_mut_log().remove(&self.field);
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
                rate_limit_secs = 30
            );
        };

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::LogfmtConfig;
    use crate::{
        event::{LogEvent, Value},
        test_util,
        topology::config::{TransformConfig, TransformContext},
        Event,
    };

    fn parse_log(text: &str, drop_field: bool, types: &[(&str, &str)]) -> LogEvent {
        let event = Event::from(text);

        let rt = test_util::runtime();
        let mut parser = LogfmtConfig {
            field: None,
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
        }
        .build(TransformContext::new_test(rt.executor()))
        .unwrap();

        parser.transform(event).unwrap().into_log()
    }

    #[test]
    fn logfmt_adds_parsed_field_to_event() {
        let log = parse_log("status=1234 time=\"5678\"", false, &[]);

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn logfmt_does_drop_parsed_field() {
        let log = parse_log("status=1234 time=5678", true, &[]);

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_none());
    }

    #[test]
    fn logfmt_does_not_drop_same_name_parsed_field() {
        let log = parse_log("status=1234 message=yes", true, &[]);

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
    }

    #[test]
    fn logfmt_coerces_fields_to_types() {
        let log = parse_log(
            "code=1234 flag=yes number=42.3 rest=word",
            false,
            &[("flag", "bool"), ("code", "integer"), ("number", "float")],
        );

        assert_eq!(log[&"number".into()], Value::Float(42.3));
        assert_eq!(log[&"flag".into()], Value::Boolean(true));
        assert_eq!(log[&"code".into()], Value::Integer(1234));
        assert_eq!(log[&"rest".into()], Value::Bytes("word".into()));
    }

    #[test]
    fn heroku_router_message() {
        let log = parse_log(
            r#"at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#,
            true,
            &[("status", "integer"), ("bytes", "integer")],
        );

        assert_eq!(log[&"at".into()], "info".into());
        assert_eq!(log[&"method".into()], "GET".into());
        assert_eq!(log[&"path".into()], "/cart_link".into());
        assert_eq!(
            log[&"request_id".into()],
            "05726858-c44e-4f94-9a20-37df73be9006".into(),
        );
        assert_eq!(log[&"fwd".into()], "73.75.38.87".into());
        assert_eq!(log[&"dyno".into()], "web.1".into());
        assert_eq!(log[&"connect".into()], "1ms".into());
        assert_eq!(log[&"service".into()], "22ms".into());
        assert_eq!(log[&"status".into()], Value::Integer(304));
        assert_eq!(log[&"bytes".into()], Value::Integer(656));
        assert_eq!(log[&"protocol".into()], "http".into());
    }

    #[test]
    fn logfmt_handles_herokus_weird_octothorpes() {
        let log = parse_log("source=web.1 dyno=heroku.2808254.d97d0ea7-cf3d-411b-b453-d2943a50b456 sample#memory_total=21.00MB sample#memory_rss=21.22MB sample#memory_cache=0.00MB sample#memory_swap=0.00MB sample#memory_pgpgin=348836pages sample#memory_pgpgout=343403pages", true, &[]);

        assert_eq!(log[&"source".into()], "web.1".into());
        assert_eq!(
            log[&"dyno".into()],
            "heroku.2808254.d97d0ea7-cf3d-411b-b453-d2943a50b456".into()
        );
        assert_eq!(log[&"sample#memory_total".into()], "21.00MB".into());
        assert_eq!(log[&"sample#memory_rss".into()], "21.22MB".into());
        assert_eq!(log[&"sample#memory_cache".into()], "0.00MB".into());
        assert_eq!(log[&"sample#memory_swap".into()], "0.00MB".into());
        assert_eq!(log[&"sample#memory_pgpgin".into()], "348836pages".into());
        assert_eq!(log[&"sample#memory_pgpgout".into()], "343403pages".into());
    }
}
