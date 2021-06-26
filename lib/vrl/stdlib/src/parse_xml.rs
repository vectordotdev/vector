use std::{
    borrow::Cow,
    collections::{btree_map::Entry, BTreeMap},
};
use vrl::prelude::*;

use bytes::Bytes;
use regex::{Regex, RegexBuilder};
use roxmltree::{Document, Node, NodeType};

struct ParseXmlConfig<'a> {
    /// Include XML attributes.
    include_attr: bool,
    /// XML attribute prefix, e.g. `<a href="test">` -> `{a: { "@href": "test }}`. Default: "@".
    attr_prefix: Cow<'a, str>,
    /// Key to use for text nodes. Default: "text".
    text_prop_name: Cow<'a, str>,
    /// Parse "true" or "false" as booleans. Default: true.
    parse_bools: bool,
    /// Parse "null" as null. Default: true.
    parse_nulls: bool,
    /// Parse numeric values as integers/floats. Default: true.
    parse_numbers: bool,
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

                parse_xml_to_json!(value, string_numbers: true)
            "#},
            result: Ok(
                r#"{"book": { "year": "2005", "author": "J K. Rowling", "category": "CHILDREN", "title": { "lang": "en", "value": "Harry Potter" } } }"#,
            ),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        let trim = arguments.optional("trim");
        let include_attr = arguments.optional("include_attr");
        let attr_prefix = arguments.optional("attr_prefix");
        let text_prop_name = arguments.optional("text_prop_name");
        let parse_bools = arguments.optional("parse_bools");
        let parse_nulls = arguments.optional("parse_nulls");
        let parse_numbers = arguments.optional("parse_numbers");

        Ok(Box::new(ParseXmlFn {
            trim,
            value,
            include_attr,
            attr_prefix,
            text_prop_name,
            parse_bools,
            parse_nulls,
            parse_numbers,
        }))
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
                keyword: "text_prop_name",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "parse_bools",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "parse_nulls",
                kind: kind::BOOLEAN,
                required: false,
            },
            Parameter {
                keyword: "parse_numbers",
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
    text_prop_name: Option<Box<dyn Expression>>,
    parse_bools: Option<Box<dyn Expression>>,
    parse_nulls: Option<Box<dyn Expression>>,
    parse_numbers: Option<Box<dyn Expression>>,
}

impl Expression for ParseXmlFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;

        let trim = match &self.trim {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => true,
        };

        let include_attr = match &self.include_attr {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => false,
        };

        let attr_prefix = match &self.attr_prefix {
            Some(expr) => Cow::from(expr.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned()),
            None => Cow::from("@"),
        };

        let text_prop_name = match &self.text_prop_name {
            Some(expr) => Cow::from(expr.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned()),
            None => Cow::from("text"),
        };

        let parse_bools = match &self.parse_bools {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => true,
        };

        let parse_nulls = match &self.parse_nulls {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => true,
        };

        let parse_numbers = match &self.parse_numbers {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => true,
        };

        let config = ParseXmlConfig {
            include_attr,
            attr_prefix,
            text_prop_name,
            parse_bools,
            parse_nulls,
            parse_numbers,
        };

        // Trim whitespace around XML elements, if applicable.
        let parse = if trim { trim_xml(&string) } else { string };

        let doc = Document::parse(&parse).map_err(|e| format!("unable to parse xml: {}", e))?;
        let value = process_node(doc.root(), &config);

        Ok(value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        type_def()
    }
}

fn inner_kind() -> Kind {
    Kind::Object
}

fn type_def() -> TypeDef {
    TypeDef::new()
        .fallible()
        .bytes()
        .add_object::<(), Kind>(map! { (): inner_kind() })
}

/// Process a text node, and return the correct `Value` type based on config.
fn process_text<'a>(text: &'a str, config: &ParseXmlConfig<'a>) -> Value {
    match text {
        // Parse nulls.
        "" | "null" if config.parse_nulls => Value::Null,
        // Parse bools.
        "true" if config.parse_bools => true.into(),
        "false" if config.parse_bools => false.into(),
        // String numbers.
        _ if !config.parse_numbers => text.into(),
        // Parse numbers, falling back to string.
        _ => {
            // Attempt an integer first (effectively a subset of float).
            if let Ok(v) = text.parse::<i64>() {
                return v.into();
            }

            // Then a float.
            if let Ok(v) = text.parse::<f64>() {
                return v.into();
            }

            // Fall back to string.
            text.into()
        }
    }
}

/// Process an XML node, and return a VRL `Value`.
fn process_node<'a>(node: Node, config: &ParseXmlConfig<'a>) -> Value {
    // Helper to recurse over a `Node`s children, and build an object.
    let recurse = |node: Node| -> BTreeMap<String, Value> {
        let mut map = BTreeMap::new();

        for n in node.children().into_iter().filter(|n| !n.is_comment()) {
            // Use the default tag name if blank.
            let name = if n.tag_name().name() == "" {
                config.text_prop_name.to_string()
            } else {
                n.tag_name().name().to_string()
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
        NodeType::Element => match node.children().count() {
            // If there's only one element, and attributes are ignored, 'flatten' the object.
            1 if !config.include_attr => process_node(
                node.children()
                    .into_iter()
                    .next()
                    .expect("expected 1 XML node"),
                config,
            ),
            // If attributes are included, irrespective of children, expand into keys.
            _ if config.include_attr => {
                let mut map = recurse(node);

                for attr in node.attributes() {
                    map.insert(
                        format!("{}{}", config.attr_prefix, attr.name()),
                        Value::Bytes(Bytes::from(attr.value().to_string())),
                    );
                }

                Value::Object(map)
            }
            // Continue recursing into children.
            _ => Value::Object(recurse(node)),
        },
        NodeType::Text => process_text(node.text().expect("expected XML text node"), config),
        // Ignore comments and processing instructions
        _ => Value::Null,
    }
}

fn trim_xml<'a>(xml: &'a Cow<str>) -> Cow<'a, str> {
    lazy_static::lazy_static! {
        static ref RE: Regex = RegexBuilder::new(r">\s+?<")
            .multi_line(true)
            .build()
            .expect("trim regex failed");
    }

    RE.replace_all(xml, "><")
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

        ignore_attr_by_default {
            args: func_args![ value: r#"<a href="https://vector.dev">test</a>"# ],
            want: Ok(value!({ "a": "test" })),
            tdef: type_def(),
        }

        include_attr {
            args: func_args![ value: r#"<a href="https://vector.dev">test</a>"#, include_attr: true ],
            want: Ok(value!({ "a": { "@href": "https://vector.dev", "text": "test" } })),
            tdef: type_def(),
        }

        include_attr_no_attributes {
            args: func_args![ value: r#"<a>test</a>"#, include_attr: true ],
            want: Ok(value!({ "a": { "text": "test" } })),
            tdef: type_def(),
        }

        custom_text_prop_name {
            args: func_args![ value: r#"<b>test</b>"#, include_attr: true, text_prop_name: "node" ],
            want: Ok(value!({ "b": { "node": "test" } })),
            tdef: type_def(),
        }

        nested_object {
            args: func_args![ value: r#"<a><b>one</b><c>two</c></a>"# ],
            want: Ok(value!({ "a": { "b": "one", "c": "two" } })),
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
    ];
}
