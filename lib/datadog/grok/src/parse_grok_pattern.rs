use lalrpop_util::{lalrpop_mod, ParseError};

use crate::{ast::GrokPattern, lexer::Lexer};

lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    parser
);

/// Parses grok patterns as %{MATCHER:FIELD:FILTER}
#[allow(dead_code)] // will be used in the follow-up PRs
pub fn parse_grok_pattern(input: &str) -> Result<GrokPattern, String> {
    let lexer = Lexer::new(input);
    parser::GrokFilterParser::new()
        .parse(input, lexer)
        .map_err(|e| match e {
            ParseError::User { error } => error.to_string(),
            _ => format!("invalid grok pattern: {}", input),
        })
}

#[cfg(test)]
mod tests {
    use lookup::{LookupBuf, SegmentBuf};
    use vrl_compiler::Value;

    use super::*;
    use crate::ast::{Destination, Function, FunctionArgument};

    fn from_path_segments(path_segments: Vec<&str>) -> LookupBuf {
        LookupBuf::from_segments(
            path_segments
                .iter()
                .map(|&s| s.into())
                .collect::<Vec<SegmentBuf>>(),
        )
    }

    #[test]
    fn parse_grok_filter() {
        let input = r#"%{date:e-http.status.abc[".\""]:integer("a. df",.123,1.23e-32, true, null, 123e-5)}"#;
        let parsed = parse_grok_pattern(input).unwrap_or_else(|error| {
            panic!("Problem parsing grok: {:?}", error);
        });
        assert_eq!(parsed.match_fn.name, "date");
        let destination = parsed.destination.unwrap();
        assert_eq!(
            destination.path,
            from_path_segments(vec!["e-http", "status", "abc", r#".""#])
        );
        let filter = destination.filter_fn.unwrap();
        assert_eq!(filter.name, "integer");
        let args = filter.args.unwrap();
        let expected_args = vec![
            "a. df".into(),
            0.123.into(),
            1.23e-32_f64.into(),
            true.into(),
            Value::Null,
            123e-5.into(),
        ];
        for (i, arg) in args.iter().enumerate() {
            match arg {
                FunctionArgument::Arg(arg) => assert_eq!(arg, expected_args.get(i).unwrap()),
                _ => panic!("failed to parse arguments"),
            };
        }
    }

    #[test]
    fn empty_field() {
        let input = r#"%{data:}"#;
        let parsed = parse_grok_pattern(input).unwrap_or_else(|error| {
            panic!("Problem parsing grok: {:?}", error);
        });
        assert_eq!(parsed.destination, None);
    }

    #[test]
    fn escaped_quotes() {
        let input = r#"%{data:field:filter("escaped \"quotes\"")}"#;
        let parsed = parse_grok_pattern(input).unwrap_or_else(|error| {
            panic!("Problem parsing grok: {:?}", error);
        });
        assert_eq!(
            parsed.destination,
            Some(Destination {
                path: LookupBuf::from("field"),
                filter_fn: Some(Function {
                    name: "filter".to_string(),
                    args: Some(vec![FunctionArgument::Arg(r#"escaped "quotes""#.into())])
                })
            })
        );
    }

    #[test]
    fn empty_field_with_filter() {
        let input = r#"%{data::json}"#;
        let parsed = parse_grok_pattern(input).unwrap_or_else(|error| {
            panic!("Problem parsing grok: {:?}", error);
        });
        assert_eq!(
            parsed.destination,
            Some(Destination {
                path: LookupBuf::root(),
                filter_fn: Some(Function {
                    name: "json".to_string(),
                    args: None,
                })
            })
        );
    }

    #[test]
    fn invalid_escape() {
        let input = r#"%{data::json("\:")}"#;
        assert_eq!(
            parse_grok_pattern(input).expect_err("must be an invalid escape error"),
            "invalid escape literal '\\:'"
        );
    }

    #[test]
    fn escaped_new_line() {
        let input = r#"%{data::array("\\n")}"#;
        let parsed = parse_grok_pattern(input).unwrap_or_else(|error| {
            panic!("Problem parsing grok: {:?}", error);
        });
        assert_eq!(
            parsed.destination,
            Some(Destination {
                path: LookupBuf::root(),
                filter_fn: Some(Function {
                    name: "array".to_string(),
                    args: Some(vec![FunctionArgument::Arg("\n".into())]),
                })
            })
        );
    }
}
