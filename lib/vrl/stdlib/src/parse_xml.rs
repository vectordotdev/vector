use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
};

use ::value::Value;
use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use roxmltree::{Document, Node, NodeType};
use rust_decimal::prelude::Zero;
use vrl::prelude::*;

/// Used to keep Clippy's `too_many_argument` check happy.
#[derive(Debug)]
struct ParseOptions {
    trim: Option<Value>,
    include_attr: Option<Value>,
    attr_prefix: Option<Value>,
    text_key: Option<Value>,
    always_use_text_key: Option<Value>,
    parse_bool: Option<Value>,
    parse_null: Option<Value>,
    parse_number: Option<Value>,
}

fn parse_xml(value: Value, options: ParseOptions) -> Resolved {
    let string = value.try_bytes_utf8_lossy()?;
    let trim = match options.trim {
        Some(value) => value.try_boolean()?,
        None => true,
    };
    let include_attr = match options.include_attr {
        Some(value) => value.try_boolean()?,
        None => true,
    };
    let attr_prefix = match options.attr_prefix {
        Some(value) => Cow::from(value.try_bytes_utf8_lossy()?.into_owned()),
        None => Cow::from("@"),
    };
    let text_key = match options.text_key {
        Some(value) => Cow::from(value.try_bytes_utf8_lossy()?.into_owned()),
        None => Cow::from("text"),
    };
    let always_use_text_key = match options.always_use_text_key {
        Some(value) => value.try_boolean()?,
        None => false,
    };
    let parse_bool = match options.parse_bool {
        Some(value) => value.try_boolean()?,
        None => true,
    };
    let parse_null = match options.parse_null {
        Some(value) => value.try_boolean()?,
        None => true,
    };
    let parse_number = match options.parse_number {
        Some(value) => value.try_boolean()?,
        None => true,
    };
    let config = ParseXmlConfig {
        include_attr,
        attr_prefix,
        text_key,
        always_use_text_key,
        parse_bool,
        parse_null,
        parse_number,
    };
    // Trim whitespace around XML elements, if applicable.
    let parse = if trim { trim_xml(&string) } else { string };
    let doc = Document::parse(&parse).map_err(|e| format!("unable to parse xml: {e}"))?;
    let value = process_node(doc.root(), &config);
    Ok(value)
}

struct ParseXmlConfig<'a> {
    /// Include XML attributes. Default: true,
    include_attr: bool,
    /// XML attribute prefix, e.g. `<a href="test">` -> `{a: { "@href": "test }}`. Default: "@".
    attr_prefix: Cow<'a, str>,
    /// Key to use for text nodes when attributes are included. Default: "text".
    text_key: Cow<'a, str>,
    /// Always use text default (instead of flattening). Default: false.
    always_use_text_key: bool,
    /// Parse "true" or "false" as booleans. Default: true.
    parse_bool: bool,
    /// Parse "null" as null. Default: true.
    parse_null: bool,
    /// Parse numeric values as integers/floats. Default: true.
    parse_number: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ParseXml;

impl Function for ParseXml {
    fn identifier(&self) -> &'static str {
        "parse_xml"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse XML",
            source: indoc! {r#"
				value = s'<book category="CHILDREN"><title lang="en">Harry Potter</title><author>J K. Rowling</author><year>2005</year></book>';

				parse_xml!(value, text_key: "value", parse_number: false)
            "#},
            result: Ok(
                r#"{ "book": { "@category": "CHILDREN", "author": "J K. Rowling", "title": { "@lang": "en", "value": "Harry Potter" }, "year": "2005" } }"#,
            ),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        let trim = arguments.optional("trim");
        let include_attr = arguments.optional("include_attr");
        let attr_prefix = arguments.optional("attr_prefix");
        let text_key = arguments.optional("text_key");
        let always_use_text_key = arguments.optional("always_use_text_key");
        let parse_bool = arguments.optional("parse_bool");
        let parse_null = arguments.optional("parse_null");
        let parse_number = arguments.optional("parse_number");

        Ok(ParseXmlFn {
            value,
            trim,
            include_attr,
            attr_prefix,
            text_key,
            always_use_text_key,
            parse_bool,
            parse_null,
            parse_number,
        }
        .as_expr())
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "trim",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "include_attr",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "attr_prefix",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "text_key",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "always_use_text_key",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "parse_bool",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "parse_null",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "parse_number",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct ParseXmlFn {
    value: Box<dyn Expression>,

    trim: Option<Box<dyn Expression>>,
    include_attr: Option<Box<dyn Expression>>,
    attr_prefix: Option<Box<dyn Expression>>,
    text_key: Option<Box<dyn Expression>>,
    always_use_text_key: Option<Box<dyn Expression>>,
    parse_bool: Option<Box<dyn Expression>>,
    parse_null: Option<Box<dyn Expression>>,
    parse_number: Option<Box<dyn Expression>>,
}

impl FunctionExpression for ParseXmlFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        let options = ParseOptions {
            trim: self
                .trim
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            include_attr: self
                .include_attr
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            attr_prefix: self
                .attr_prefix
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            text_key: self
                .text_key
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            always_use_text_key: self
                .always_use_text_key
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            parse_bool: self
                .parse_bool
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            parse_null: self
                .parse_null
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,

            parse_number: self
                .parse_number
                .as_ref()
                .map(|expr| expr.resolve(ctx))
                .transpose()?,
        };

        parse_xml(value, options)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        type_def()
    }
}

fn type_def() -> TypeDef {
    TypeDef::bytes()
        .or_object(Collection::from_unknown(inner_kind()))
        .fallible()
}

fn inner_kind() -> Kind {
    Kind::object(Collection::any())
}

/// Process an XML node, and return a VRL `Value`.
fn process_node<'a>(node: Node, config: &ParseXmlConfig<'a>) -> Value {
    // Helper to recurse over a `Node`s children, and build an object.
    let recurse = |node: Node| -> BTreeMap<String, Value> {
        let mut map = BTreeMap::new();

        // Expand attributes, if required.
        if config.include_attr {
            for attr in node.attributes() {
                map.insert(
                    format!("{}{}", config.attr_prefix, attr.name()),
                    attr.value().into(),
                );
            }
        }

        for n in node
            .children()
            .into_iter()
            .filter(|n| n.is_element() || n.is_text())
        {
            let name = match n.node_type() {
                NodeType::Element => n.tag_name().name().to_string(),
                NodeType::Text => config.text_key.to_string(),
                _ => unreachable!("shouldn't be other XML nodes"),
            };

            // Transform the node into a VRL `Value`.
            let value = process_node(n, config);

            // If the key already exists, add it. Otherwise, insert.
            match map.entry(name) {
                Entry::Occupied(mut entry) => {
                    let v = entry.get_mut();

                    // Push a value onto the existing array, or wrap in a `Value::Array`.
                    match v {
                        Value::Array(v) => v.push(value),
                        v => {
                            let prev = std::mem::replace(v, Value::Array(Vec::with_capacity(2)));
                            if let Value::Array(v) = v {
                                v.extend_from_slice(&[prev, value]);
                            }
                        }
                    };
                }
                Entry::Vacant(entry) => {
                    entry.insert(value);
                }
            }
        }

        map
    };

    match node.node_type() {
        NodeType::Root => Value::Object(recurse(node)),

        NodeType::Element => {
            match (
                config.always_use_text_key,
                node.attributes().len().is_zero(),
            ) {
                // If the node has attributes, *always* recurse to expand default keys.
                (_, false) if config.include_attr => Value::Object(recurse(node)),
                // If a text key should be used, always recurse.
                (true, true) => Value::Object(recurse(node)),
                // Otherwise, check the node count to determine what to do.
                _ => match node.children().count() {
                    // For a single node, 'flatten' the object if necessary.
                    1 => {
                        // Expect a single element.
                        let node = node
                            .children()
                            .into_iter()
                            .next()
                            .expect("expected 1 XML node");

                        // If the node is an element, treat it as an object.
                        if node.is_element() {
                            let mut map = BTreeMap::new();

                            map.insert(
                                node.tag_name().name().to_string(),
                                Value::Object(recurse(node)),
                            );

                            Value::Object(map)
                        } else {
                            // Otherwise, 'flatten' the object by continuing processing.
                            process_node(node, config)
                        }
                    }
                    // For 2+ nodes, expand.
                    _ => Value::Object(recurse(node)),
                },
            }
        }
        NodeType::Text => process_text(node.text().expect("expected XML text node"), config),
        _ => unreachable!("shouldn't be other XML nodes"),
    }
}

/// Process a text node, and return the correct `Value` type based on config.
fn process_text<'a>(text: &'a str, config: &ParseXmlConfig<'a>) -> Value {
    match text {
        // Parse nulls.
        "" | "null" if config.parse_null => Value::Null,
        // Parse bools.
        "true" if config.parse_bool => true.into(),
        "false" if config.parse_bool => false.into(),
        // String numbers.
        _ if !config.parse_number => text.into(),
        // Parse numbers, falling back to string.
        _ => {
            // Attempt an integer first (effectively a subset of float).
            if let Ok(v) = text.parse::<i64>() {
                return v.into();
            }

            // Then a float.
            if let Ok(v) = text.parse::<f64>() {
                return Value::from_f64_or_zero(v);
            }

            // Fall back to string.
            text.into()
        }
    }
}

static XML_RE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r">\s+?<")
        .multi_line(true)
        .build()
        .expect("trim regex failed")
});

#[inline]
fn trim_xml(xml: &str) -> Cow<str> {
    XML_RE.replace_all(xml, "><")
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_xml => ParseXml;

        simple_text {
            args: func_args![ value: r#"<a>test</a>"# ],
            want: Ok(value!({ "a": "test" })),
            tdef: type_def(),
        }

        include_attr {
            args: func_args![ value: r#"<a href="https://vector.dev">test</a>"# ],
            want: Ok(value!({ "a": { "@href": "https://vector.dev", "text": "test" } })),
            tdef: type_def(),
        }

        exclude_attr {
            args: func_args![ value: r#"<a href="https://vector.dev">test</a>"#, include_attr: false ],
            want: Ok(value!({ "a": "test" })),
            tdef: type_def(),
        }

        custom_text_key {
            args: func_args![ value: r#"<b>test</b>"#, text_key: "node", always_use_text_key: true ],
            want: Ok(value!({ "b": { "node": "test" } })),
            tdef: type_def(),
        }

        // https://github.com/vectordotdev/vector/issues/11901
        include_attributes_if_single_node {
            args: func_args![ value: r#"<root><node attr="value"><message>foo</message></node></root>"# ],
            want: Ok(value!({ "root": { "node": { "@attr": "value", "message": "foo" } } })),
            tdef: type_def(),
        }

        // https://github.com/vectordotdev/vector/issues/11901
        include_attributes_multiple_children {
            args: func_args![ value: r#"<root><node attr="value"><message>bar</message></node><node attr="value"><message>baz</message></node></root>"#],
            want: Ok(value!({"root":{ "node":[ { "@attr": "value", "message": "bar" }, { "@attr": "value", "message": "baz" } ] } })),
            tdef: type_def(),
        }

        nested_object {
            args: func_args![ value: r#"<a attr="value"><b>one</b><c>two</c></a>"# ],
            want: Ok(value!({ "a": { "@attr": "value", "b": "one", "c": "two" } })),
            tdef: type_def(),
        }

        nested_object_array {
            args: func_args![ value: r#"<a><b>one</b><b>two</b></a>"# ],
            want: Ok(value!({ "a": { "b": ["one", "two"] } })),
            tdef: type_def(),
        }

        header_and_comments {
            args: func_args![ value: indoc!{r#"
                <?xml version="1.0" encoding="ISO-8859-1"?>
                <!-- Example found somewhere in the deep depths of the web -->
                <note>
                    <to>Tove</to>
                    <!-- Randomly inserted inner comment -->
                    <from>Jani</from>
                    <heading>Reminder</heading>
                    <body>Don't forget me this weekend!</body>
                </note>

                <!-- Could literally be placed anywhere -->
            "#}],
            want: Ok(value!(
                {
                    "note": {
                        "to": "Tove",
                        "from": "Jani",
                        "heading": "Reminder",
                        "body": "Don't forget me this weekend!"
                    }
                }
            )),
            tdef: type_def(),
        }

        header_inside_element {
            args: func_args![ value: r#"<p><?xml?>text123</p>"# ],
            want: Ok(value!(
                {
                    "p": {
                        "text": "text123"
                    }
                }
            )),
            tdef: type_def(),
        }

        mixed_types {
            args: func_args![ value: indoc!{r#"
                <?xml version="1.0" encoding="ISO-8859-1"?>
                <!-- Mixed types -->
                <data>
                    <!-- Booleans -->
                    <item>true</item>
                    <item>false</item>
                    <!-- String -->
                    <item>string!</item>
                    <!-- Empty object -->
                    <item />
                    <!-- Literal value "null" -->
                    <item>null</item>
                    <!-- Integer -->
                    <item>1</item>
                    <!-- Float -->
                    <item>1.0</item>
                </data>
            "#}],
            want: Ok(value!(
                {
                    "data": {
                        "item": [
                            true,
                            false,
                            "string!",
                            {},
                            null,
                            1,
                            1.0
                        ]
                    }
                }
            )),
            tdef: type_def(),
        }

        just_strings {
            args: func_args![ value: indoc!{r#"
                <?xml version="1.0" encoding="ISO-8859-1"?>
                <!-- All scalar types are just strings -->
                <data>
                    <item>true</item>
                    <item>false</item>
                    <item>string!</item>
                    <!-- Still an empty object -->
                    <item />
                    <item>null</item>
                    <item>1</item>
                    <item>1.0</item>
                </data>
            "#}, parse_null: false, parse_bool: false, parse_number: false],
            want: Ok(value!(
                {
                    "data": {
                        "item": [
                            "true",
                            "false",
                            "string!",
                            {},
                            "null",
                            "1",
                            "1.0"
                        ]
                    }
                }
            )),
            tdef: type_def(),
        }

        untrimmed {
            args: func_args![ value: "<root>  <a>test</a>  </root>", trim: false ],
            want: Ok(value!(
                {
                    "root": {
                        "a": "test",
                        "text": ["  ", "  "],
                    }
                }
            )),
            tdef: type_def(),
        }

        invalid_token {
            args: func_args![ value: "true" ],
            want: Err("unable to parse xml: unknown token at 1:1"),
            tdef: type_def(),
        }

        flat_parent_property {
            args: func_args![ value: indoc!{r#"
                <?xml version="1.0" encoding="UTF-8"?>
                <MY_XML>
                  <property1>
                    <property1_a>a</property1_a>
                    <property1_b>b</property1_b>
                    <property1_c>c</property1_c>
                  </property1>
                  <property2>
                    <property2_object>
                      <property2a_a>a</property2a_a>
                      <property2a_b>b</property2a_b>
                      <property2a_c>c</property2a_c>
                    </property2_object>
                  </property2>
                </MY_XML>
            "#}],
            want: Ok(value!(
                {
                  "MY_XML": {
                    "property1": {
                      "property1_a": "a",
                      "property1_b": "b",
                      "property1_c": "c"
                    },
                    "property2": {
                      "property2_object": {
                        "property2a_a": "a",
                        "property2a_b": "b",
                        "property2a_c": "c"
                      }
                    }
                  }
                }
            )),
            tdef: type_def(),
        }

        nested_parent_property {
            args: func_args![ value: indoc!{r#"
                <?xml version="1.0" encoding="UTF-8"?>
                <MY_XML>
                  <property1>
                    <property1_a>a</property1_a>
                    <property1_b>b</property1_b>
                    <property1_c>c</property1_c>
                  </property1>
                  <property2>
                    <property2_object>
                      <property2a_a>a</property2a_a>
                      <property2a_b>b</property2a_b>
                      <property2a_c>c</property2a_c>
                    </property2_object>
                    <property2_object>
                      <property2a_a>a</property2a_a>
                      <property2a_b>b</property2a_b>
                      <property2a_c>c</property2a_c>
                    </property2_object>
                  </property2>
                </MY_XML>
            "#}],
            want: Ok(value!(
                {
                  "MY_XML": {
                    "property1": {
                      "property1_a": "a",
                      "property1_b": "b",
                      "property1_c": "c"
                    },
                    "property2": {
                      "property2_object": [
                        {
                          "property2a_a": "a",
                          "property2a_b": "b",
                          "property2a_c": "c"
                        },
                        {
                          "property2a_a": "a",
                          "property2a_b": "b",
                          "property2a_c": "c"
                        }
                      ]
                    }
                  }
                }
            )),
            tdef: type_def(),
        }
    ];

    #[test]
    fn test_kind() {
        let state = state::TypeState::default();

        let func = ParseXmlFn {
            value: value!(true).into_expression(),
            trim: None,
            include_attr: None,
            attr_prefix: None,
            text_key: None,
            always_use_text_key: None,
            parse_bool: None,
            parse_null: None,
            parse_number: None,
        };

        let type_def = func.type_def(&state);

        assert!(type_def.is_fallible());
        assert!(!type_def.is_exact());
        assert!(type_def.contains_bytes());
        assert!(type_def.contains_object());

        let object1 = type_def.as_object().unwrap();

        assert!(object1.known().is_empty());
        assert!(object1.unknown_kind().contains_object());

        let object2 = object1.unknown_kind().as_object().cloned().unwrap();

        assert!(object2.known().is_empty());
        assert!(object2.unknown_kind().is_any());
    }
}
