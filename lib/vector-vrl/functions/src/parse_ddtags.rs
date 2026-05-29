use std::collections::BTreeMap;
use std::sync::LazyLock;

use vrl::prelude::*;

static DEFAULT_MULTIVALUE: LazyLock<Value> = LazyLock::new(|| Value::Boolean(true));

static PARAMETERS: LazyLock<Vec<Parameter>> = LazyLock::new(|| {
    vec![
        Parameter::required(
            "value",
            kind::BYTES,
            "The Datadog tag string to parse (comma-separated key:value pairs).",
        ),
        Parameter::optional(
            "multivalue",
            kind::BOOLEAN,
            "If true (default), all values are wrapped in arrays to support duplicate keys. If false, the first value for each key wins.",
        )
        .default(&DEFAULT_MULTIVALUE),
    ]
});

/// Iterate over the tags in a ddtag string, yielding `(key, value)` pairs.
/// Skips empty segments and empty keys. Splits on the first colon only.
fn iter_tags(input: &str) -> impl Iterator<Item = (&str, Value)> {
    input.split(',').filter_map(|segment| {
        let segment = segment.trim();
        if segment.is_empty() {
            return None;
        }
        let (key, val) = match segment.find(':') {
            Some(pos) => (segment[..pos].trim(), Value::from(segment[pos + 1..].trim())),
            None => (segment, Value::Boolean(true)),
        };
        if key.is_empty() { None } else { Some((key, val)) }
    })
}

fn collect_multivalue(input: &str) -> Value {
    let mut map: BTreeMap<KeyString, Vec<Value>> = BTreeMap::new();
    for (key, val) in iter_tags(input) {
        map.entry(KeyString::from(key)).or_default().push(val);
    }
    Value::Object(map.into_iter().map(|(k, v)| (k, Value::Array(v))).collect())
}

fn collect_single_value(input: &str) -> Value {
    let mut map = ObjectMap::new();
    for (key, val) in iter_tags(input) {
        map.entry(KeyString::from(key)).or_insert(val);
    }
    Value::Object(map)
}

fn parse_ddtags_impl(value: Value, multivalue: Value) -> Resolved {
    let input = value.try_bytes_utf8_lossy()?;
    let multivalue = multivalue.try_boolean()?;

    Ok(if multivalue {
        collect_multivalue(&input)
    } else {
        collect_single_value(&input)
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ParseDdtags;

impl Function for ParseDdtags {
    fn identifier(&self) -> &'static str {
        "parse_ddtags"
    }

    fn summary(&self) -> &'static str {
        "Parses a Datadog tag string into an object."
    }

    fn usage(&self) -> &'static str {
        indoc! {r#"
            Parses the `value` as a Datadog tag string — comma-separated `key:value` pairs
            such as `"env:prod,host:server1,host:server2"`.

            When `multivalue` is `true` (the default), every value is wrapped in an array so
            that duplicate keys are preserved:

                parse_ddtags!("host:a,host:b")  =>  {"host": ["a", "b"]}

            When `multivalue` is `false`, the first encountered value for each key wins and
            values are stored as plain strings:

                parse_ddtags!("host:a,host:b", multivalue: false)  =>  {"host": "a"}

            * Tags without a colon become standalone keys with a boolean `true` value.
            * Values containing colons (e.g. URLs, ARNs) are handled correctly — only the
              first colon is used as the key/value separator.
            * Whitespace around keys and values is trimmed.
            * Empty segments from leading, trailing, or consecutive commas are ignored.
        "#}
    }

    fn category(&self) -> &'static str {
        Category::Parse.as_ref()
    }

    fn internal_failure_reasons(&self) -> &'static [&'static str] {
        &["`value` is not a string."]
    }

    fn return_kind(&self) -> u16 {
        kind::OBJECT
    }

    fn return_rules(&self) -> &'static [&'static str] {
        &[
            "The function is fallible — it raises an error if `value` is not a string.",
            "When `multivalue` is `true`, returns an object mapping each key to an array of strings (or booleans for standalone keys).",
            "When `multivalue` is `false`, returns an object mapping each key to a single string (or boolean). Duplicate keys are resolved in favor of the first occurrence.",
        ]
    }

    fn parameters(&self) -> &'static [Parameter] {
        PARAMETERS.as_slice()
    }

    fn examples(&self) -> &'static [Example] {
        &[
            example! {
                title: "Parse Datadog tags (multivalue, default)",
                source: r#"parse_ddtags!("env:prod,host:server1")"#,
                result: Ok(r#"{"env": ["prod"], "host": ["server1"]}"#),
            },
            example! {
                title: "Parse Datadog tags with duplicate keys",
                source: r#"parse_ddtags!("env:prod,host:a,host:b")"#,
                result: Ok(r#"{"env": ["prod"], "host": ["a", "b"]}"#),
            },
            example! {
                title: "Parse Datadog tags (single-value mode)",
                source: r#"parse_ddtags!("env:prod,host:a,host:b", multivalue: false)"#,
                result: Ok(r#"{"env": "prod", "host": "a"}"#),
            },
            example! {
                title: "Standalone keys and colons in values",
                source: r#"parse_ddtags!("novalue,url:https://example.com:8080/path", multivalue: false)"#,
                result: Ok(r#"{"novalue": true, "url": "https://example.com:8080/path"}"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let multivalue = arguments.optional("multivalue");

        Ok(ParseDdtagsFn { value, multivalue }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ParseDdtagsFn {
    value: Box<dyn Expression>,
    multivalue: Option<Box<dyn Expression>>,
}

impl FunctionExpression for ParseDdtagsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let multivalue = self
            .multivalue
            .map_resolve_with_default(ctx, || DEFAULT_MULTIVALUE.clone())?;
        parse_ddtags_impl(value, multivalue)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        // Values can be strings (from key:value) or booleans (standalone keys),
        // and in multivalue mode they are wrapped in arrays.
        TypeDef::object(Collection::from_unknown(
            Kind::bytes()
                | Kind::boolean()
                | Kind::array(Collection::from_unknown(Kind::bytes() | Kind::boolean())),
        ))
        .fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrl::value;

    #[test]
    fn basic_multivalue() {
        let result =
            parse_ddtags_impl(Value::from("env:prod,host:server1"), Value::Boolean(true)).unwrap();
        assert_eq!(result, value!({"env": ["prod"], "host": ["server1"]}));
    }

    #[test]
    fn duplicate_keys_multivalue() {
        let result =
            parse_ddtags_impl(Value::from("env:prod,host:a,host:b"), Value::Boolean(true)).unwrap();
        assert_eq!(result, value!({"env": ["prod"], "host": ["a", "b"]}));
    }

    #[test]
    fn duplicate_keys_single_value_first_wins() {
        let result =
            parse_ddtags_impl(Value::from("env:prod,host:a,host:b"), Value::Boolean(false))
                .unwrap();
        assert_eq!(result, value!({"env": "prod", "host": "a"}));
    }

    #[test]
    fn standalone_key_multivalue() {
        let result =
            parse_ddtags_impl(Value::from("env:prod,standalone"), Value::Boolean(true)).unwrap();
        assert_eq!(result, value!({"env": ["prod"], "standalone": [true]}));
    }

    #[test]
    fn standalone_key_single_value() {
        let result =
            parse_ddtags_impl(Value::from("env:prod,standalone"), Value::Boolean(false)).unwrap();
        assert_eq!(result, value!({"env": "prod", "standalone": true}));
    }

    #[test]
    fn empty_input() {
        let result = parse_ddtags_impl(Value::from(""), Value::Boolean(true)).unwrap();
        assert_eq!(result, value!({}));
    }

    #[test]
    fn whitespace_trimming() {
        let result = parse_ddtags_impl(
            Value::from(" env : prod , host : server1 "),
            Value::Boolean(false),
        )
        .unwrap();
        assert_eq!(result, value!({"env": "prod", "host": "server1"}));
    }

    #[test]
    fn multiple_colons_in_value() {
        let result = parse_ddtags_impl(
            Value::from("url:http://example.com:8080/path"),
            Value::Boolean(false),
        )
        .unwrap();
        assert_eq!(result, value!({"url": "http://example.com:8080/path"}));
    }

    #[test]
    fn trailing_and_leading_commas() {
        let result =
            parse_ddtags_impl(Value::from(",env:prod,,host:a,"), Value::Boolean(false)).unwrap();
        assert_eq!(result, value!({"env": "prod", "host": "a"}));
    }

    #[test]
    fn empty_key_skipped() {
        let result =
            parse_ddtags_impl(Value::from(":value,env:prod"), Value::Boolean(false)).unwrap();
        assert_eq!(result, value!({"env": "prod"}));
    }

    #[test]
    fn realistic_ddtags() {
        let result = parse_ddtags_impl(
            Value::from(
                "env:prod,host:server1,region:us-east-1,service:web,version:1.2.3,host:server2,team:platform",
            ),
            Value::Boolean(true),
        )
        .unwrap();
        assert_eq!(
            result,
            value!({
                "env": ["prod"],
                "host": ["server1", "server2"],
                "region": ["us-east-1"],
                "service": ["web"],
                "team": ["platform"],
                "version": ["1.2.3"],
            })
        );
    }
}
