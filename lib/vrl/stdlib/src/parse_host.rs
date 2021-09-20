use std::collections::BTreeMap;
use tldextract::*;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseHost;

impl Function for ParseHost {
    fn identifier(&self) -> &'static str {
        "parse_host"
    }

    fn summary(&self) -> &'static str {
        "parse a host string"
    }

    fn usage(&self) -> &'static str {
        indoc! {r#"
            Parses the provided `value` into TLD, regiestered domain, and subdomains.

            By default included private domains.
        "#}
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "private_domains",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "parse host",
                source: r#"parse_host!("www.example.com")"#,
                result: Ok(indoc! {r#"
                    {
                        "domain": "example",
                        "subdomain": "www",
                        "suffix": "com"
                    }
                "#}),
            },
            Example {
                title: "ignore private domain",
                source: r#"parse_host!("internal.us-east-1.elb.amazonaws.com", private_domains: false)"#,
                result: Ok(indoc! {r#"
                    {
                        "domain": "amazonaws",
                        "subdomain": "internal.us-east-1.elb",
                        "suffix": "com"
                    }
                "#}),
            },
        ]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        let private_domains = arguments
            .optional("private_domains")
            .unwrap_or_else(|| expr!(true));

        Ok(Box::new(ParseHostFn {
            value,
            private_domains,
        }))
    }
}

#[derive(Debug, Clone)]
struct ParseHostFn {
    value: Box<dyn Expression>,
    private_domains: Box<dyn Expression>,
}

impl Expression for ParseHostFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;

        let private_domains = self.private_domains.resolve(ctx)?.try_boolean()?;

        let ext = TldExtractor::new(TldOption {
            private_domains,
            ..Default::default()
        });
        let tld = ext.extract(&string).unwrap();
        Ok(tld_to_value(tld))
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object(type_def())
    }
}

fn tld_to_value(tld: TldResult) -> Value {
    let mut map = BTreeMap::<&str, Value>::new();

    map.insert("domain", tld.domain.to_owned().into());
    map.insert("subdomain", tld.subdomain.to_owned().into());
    map.insert("suffix", tld.suffix.to_owned().into());

    map.into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect::<Value>()
}

fn type_def() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "domain": Kind::Bytes | Kind::Null,
        "subdomain": Kind::Bytes | Kind::Null,
        "suffix": Kind::Bytes | Kind::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_host => ParseHost;

        multiple_subdomains {
            args: func_args![value: value!("sub.asdf.example.com")],
            want: Ok(value!({
                domain: "example",
                subdomain: "sub.asdf",
                suffix: "com",
            })),
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
        }

        private {
            args: func_args![value: value!("internal.us-east-1.elb.amazonaws.com")],
            want: Ok(value!({
                domain: "internal",
                subdomain: null,
                suffix: "us-east-1.elb.amazonaws.com",
            })),
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
        }

        no_private {
            args: func_args![value: value!("internal.us-east-1.elb.amazonaws.com"), private_domains: false],
            want: Ok(value!({
                domain: "amazonaws",
                subdomain: "internal.us-east-1.elb",
                suffix: "com",
            })),
            tdef: TypeDef::new().fallible().object::<&'static str, TypeDef>(type_def()),
        }
    ];
}
