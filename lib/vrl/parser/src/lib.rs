use lalrpop_util::lalrpop_mod;
lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    parser
);

pub mod ast;
mod lex;

pub use ast::{Field, Literal, Path, PathSegment, Program};
pub use diagnostic::Span;
pub use lex::{Error, Token};

pub fn parse(input: impl AsRef<str>) -> Result<Program, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::ProgramParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
}

pub fn parse_path(input: impl AsRef<str>) -> Result<Path, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::QueryParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
        .and_then(|query| match query.target.into_inner() {
            ast::QueryTarget::External => Ok(query.path.into_inner()),
            _ => Err(Error::UnexpectedParseError(
                "unexpected query target".to_owned(),
            )),
        })
}

pub fn parse_field(input: impl AsRef<str>) -> Result<Field, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::FieldParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
}

pub fn parse_literal(input: impl AsRef<str>) -> Result<Literal, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::LiteralParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
}

pub mod test {
    pub use super::parser::TestParser as Parser;
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use ast::*;
//     use lex::Lexer;
//     use parser::{ProgramParser, TestParser};

//     mod atomics {
//         use super::*;
//         use test_case::test_case;

//         #[test]
//         fn null() {
//             let source = "_a null";
//             let lexer = Lexer::new(source);
//             let result = TestParser::new().parse(source, lexer).map(Test::null);

//             assert_eq!(result, Ok(()))
//         }

//         #[test_case(r#"foo"#, Ok("foo"))]
//         #[test_case(r#"foo bar"#, Ok("foo bar"))]
//         #[test_case(r#"bar \" \n \t baz"#, Ok("bar \" \n \t baz"))]
//         #[test_case(r#"bar \a baz"#, Err(()))]
//         fn string(source: &str, expect: Result<&str, ()>) {
//             let source = format!(r#"_a "{}""#, source);
//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::string)
//                 .map_err(|_| ());

//             let expect = expect.map(|s| s.to_owned());

//             assert_eq!(result, expect)
//         }

//         #[test_case("-100", Ok(-100))]
//         #[test_case("-41", Ok(-41))]
//         #[test_case("-2", Ok(-2))]
//         #[test_case("0", Ok(0))]
//         #[test_case("1", Ok(1))]
//         #[test_case("10", Ok(10))]
//         #[test_case("42", Ok(42))]
//         #[test_case("-0", Ok(0); "ok")]
//         #[test_case("foo", Err(()))]
//         fn integer(source: &str, expect: Result<i64, ()>) {
//             let source = format!("_a {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new().parse(&source, lexer).map(Test::integer);

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), expect)
//         }

//         #[test_case("0.0", Ok(0.0))]
//         #[test_case("1.0", Ok(1.0))]
//         #[test_case("10.12", Ok(10.12))]
//         #[test_case("-42.567", Ok(-42.567))]
//         #[test_case("foo", Err(()))]
//         fn float(source: &str, expect: Result<f64, ()>) {
//             let source = format!("_a {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::float)
//                 .map(|f| f.into_inner())
//                 .map_err(|_| ());

//             assert_eq!(result, expect)
//         }

//         #[test_case("true", Ok(true))]
//         #[test_case("false", Ok(false))]
//         #[test_case("foo", Err(()))]
//         fn boolean(source: &str, expect: Result<bool, ()>) {
//             let source = format!("_a {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::boolean)
//                 .map_err(|_| ());

//             let expect = expect.map_err(|_| ());

//             assert_eq!(result, expect)
//         }

//         #[test_case("r/foo/", Ok("/foo/"))]
//         #[test_case("r/foo \\/ bar/", Ok("/foo / bar/"))]
//         #[test_case("r/foo/i", Ok("/foo/i"))]
//         #[test_case("r/foo/ix", Ok("/foo/ix"))]
//         #[test_case("r/foo/xim", Ok("/foo/ixm"))]
//         #[test_case("r/foo/immxm", Ok("/foo/ixm"))]
//         #[test_case("r/[/", Ok("/[/"))]
//         #[test_case("r//", Ok("//"); "works")]
//         #[test_case("r//ix", Ok("//ix"); "works too")]
//         #[test_case("foo", Err(()))]
//         fn regex(source: &str, expect: Result<&str, ()>) {
//             let source = format!("_a {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new().parse(&source, lexer).map(Test::regex);

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             let expect = expect.map(|r| r.to_string());

//             assert_eq!(result.map_err(|_| ()), expect)
//         }
//     }

//     mod queries {
//         use super::*;
//         use test_case::test_case;

//         fn test(source: &str, expect: Result<&str, ()>) {
//             let source = format!("_q {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::query)
//                 .map(|node| format!("{:?}", node));

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), expect.map(ToString::to_string))
//         }

//         #[test_case(".", Ok("Query(External, Path([]))"))]
//         #[test_case(".foo", Ok("Query(External, Path([Field(Regular(foo))]))"))]
//         #[test_case(r#".a."b\"c""#, Ok(r#"Query(External, Path([Field(Regular(a)), Field(Quoted("b\"c"))]))"#); "1")]
//         fn external(source: &str, expect: Result<&str, ()>) {
//             test(source, expect)
//         }

//         #[test_case("{ foo }.foo", Ok("Query(Container(Block(Block([Variable(foo)]))), Path([Field(Regular(foo))]))"); "1")]
//         #[test_case("{ [1, 2] }[0]", Ok("Query(Container(Block(Block([Container(Array([Atom(Integer(1)), Atom(Integer(2))]))]))), Path([Index(0)]))"); "2")]
//         fn block(source: &str, expect: Result<&str, ()>) {
//             test(source, expect)
//         }

//         #[test_case("foo(a: true).bar[2]", Ok("Query(FunctionCall(foo(a: Atom(Boolean(true)))), Path([Field(Regular(bar)), Index(2)]))"); "1")]
//         fn function_call(source: &str, expect: Result<&str, ()>) {
//             test(source, expect)
//         }
//     }

//     mod containers {
//         use super::*;
//         use test_case::test_case;

//         #[test_case("[]", Ok(vec![]))]
//         #[test_case("[1]", Ok(vec![Literal::Integer(1)]))]
//         #[test_case("[1,\n2]", Ok(vec![Literal::Integer(1), Literal::Integer(2)]))]
//         #[test_case("[1, true, \"bar\",]", Ok(vec![Literal::Integer(1), Literal::Boolean(true), Literal::String("bar".to_owned())]))]
//         #[test_case("[1", Err(()))]
//         #[test_case("[1}", Err(()); "err")]
//         fn array(source: &str, expect: Result<Vec<Literal>, ()>) {
//             let source = format!("_c {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::array)
//                 .map(|array| {
//                     array
//                         .into_iter()
//                         .map(|v| match v.into_inner() {
//                             Expr::Literal(v) => v.into_inner(),
//                             v => panic!(v),
//                         })
//                         .collect::<Vec<_>>()
//                 });

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), expect)
//         }

//         #[test_case("{}", Ok(vec![]))]
//         #[test_case(r#"{ "foo": 1 }"#, Ok(vec![("foo".to_owned(), Literal::Integer(1))]))]
//         #[test_case(r#"{ "foo": 1, "bar": "baz" }"#, Ok(vec![("foo".to_owned(), Literal::Integer(1)), ("bar".to_owned(), Literal::String("baz".to_owned()))]))]
//         fn object(source: &str, expect: Result<Vec<(String, Literal)>, ()>) {
//             let source = format!("_c {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map_err(|_| ())
//                 .map(Test::object)
//                 .map(|object| {
//                     object
//                         .into_iter()
//                         .map(|(k, v)| match v.into_inner() {
//                             Expr::Literal(v) => (k.into_inner(), v.into_inner()),
//                             v => panic!(v),
//                         })
//                         .collect::<Vec<(_, _)>>()
//                 });

//             assert_eq!(result, expect)
//         }

//         #[test_case(r#"{ "foo"; "bar" }"#, Ok(vec![Literal::String("foo".to_owned()), Literal::String("bar".to_owned())]))]
//         #[test_case("{ true\n123 }", Ok(vec![Literal::Boolean(true), Literal::Integer(123)]))]
//         fn block(source: &str, expect: Result<Vec<Literal>, ()>) {
//             let source = format!("_c {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map_err(|_| ())
//                 .map(Test::block)
//                 .map(|block| {
//                     block
//                         .into_iter()
//                         .map(|v| match v.into_inner() {
//                             Expr::Literal(v) => v.into_inner(),
//                             v => panic!(v),
//                         })
//                         .collect::<Vec<Literal>>()
//                 });

//             assert_eq!(result, expect)
//         }
//     }

//     mod arithmetic {
//         use super::*;
//         use test_case::test_case;

//         #[test_case("1 + 2", "(1 + 2)")]
//         #[test_case("1 + 1 + 1", "((1 + 1) + 1)")]
//         #[test_case("2 + 1 * 1", "(2 + (1 * 1))")]
//         #[test_case("(2 + 2) * 1", "((2 + 2) * 1)")]
//         #[test_case("3 + 1 * 1", "(3 + (1 * 1))")]
//         #[test_case("4 / 1 * 1", "((4 / 1) * 1)")]
//         #[test_case("5 * 1 ?? 1", "((5 * 1) ?? 1)")]
//         #[test_case("6 || 1 ?? 1", "((6 || 1) ?? 1)")]
//         #[test_case("7 ?? 1 || 1", "(7 ?? (1 || 1))")]
//         #[test_case("(7 ?? 2) || 1", "((7 ?? 2) || 1)")]
//         #[test_case("8 && 1 || 1", "((8 && 1) || 1)")]
//         #[test_case("9 || 1 && 1", "((9 || 1) && 1)")]
//         #[test_case("false || 12", "(false || 12)")]
//         // #[test_case(r#"/foo/ || "bar""#, r#"(/foo/ || "bar")"#)]
//         fn arithmetic(source: &str, expect: &str) {
//             let source = format!("_m {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::arithmetic)
//                 .map(|node| match node.into_inner() {
//                     Expr::Op(v) => format!("{:?}", v),
//                     v => panic!(v),
//                 });

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|e| e.to_string()), Ok(expect.to_owned()))
//         }
//     }

//     mod assignment {
//         use super::*;
//         use test_case::test_case;

//         #[test_case(".foo = true", "External(.foo) = Atom(Boolean(true))")]
//         #[test_case(".foo.bar = r/baz/", "External(.foo.bar) = Atom(Regex(/baz/))")]
//         #[test_case("foo = 123", "Internal(foo) = Atom(Integer(123))")]
//         #[test_case("foo.bar = false", "Internal(foo.bar) = Atom(Boolean(false))")]
//         #[test_case("_ = true", "Noop = Atom(Boolean(true))")]
//         #[test_case("a, .b = 1.0", "Ok(Internal(a)), Err(External(.b)) = Atom(Float(1))")]
//         #[test_case("_, .b = 1.0", "Ok(Noop), Err(External(.b)) = Atom(Float(1))")]
//         #[test_case("_ = false || true", "Noop = Op((false || true))")]
//         fn assignment(source: &str, expect: &str) {
//             let source = format!("_as {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::assignment)
//                 .map(|a| format!("{:?}", a))
//                 .map_err(|e| e.to_string());

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), Ok(expect.to_owned()))
//         }
//     }

//     mod function_call {
//         use super::*;
//         use test_case::test_case;

//         #[test_case("foo()", Ok("foo"))]
//         #[test_case("foo(bar: 123)", Ok("foo(bar: Expr(Atom(Integer(123))))"))]
//         #[test_case("a(b:1,c: true)", Ok("a(b: Expr(Atom(Integer(1))), c: Expr(Atom(Boolean(true))))"); "1")]
//         #[test_case("a(\nb:1,\nc: true\n)", Ok("a(b: Expr(Atom(Integer(1))), c: Expr(Atom(Boolean(true))))"); "2")]
//         #[test_case("a(b: c = null)", Ok("a(b: Expr(Assignment(Internal(c) = Atom(Null))))"); "3")]
//         #[test_case("foo(  )", Ok("foo"); "ok")]
//         #[test_case("foo   (  )", Err(()); "fails")]
//         fn assignment(source: &str, expect: Result<&str, ()>) {
//             let source = format!("_fn {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::function_call)
//                 .map(|a| format!("{:?}", a))
//                 .map_err(|e| e.to_string());

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), expect.map(|s| s.to_owned()))
//         }
//     }

//     mod programs {
//         use super::*;
//         use test_case::test_case;

//         #[test_case("null", Ok(vec!["Atom(Null)"]))]
//         #[test_case("123", Ok(vec!["Atom(Integer(123))"]))]
//         #[test_case("12.34", Ok(vec!["Atom(Float(12.34))"]))]
//         #[test_case("r/foo/", Ok(vec!["Atom(Regex(/foo/))"]))]
//         #[test_case("true", Ok(vec!["Atom(Boolean(true))"]))]
//         fn single_expr(source: &str, expect: Result<Vec<&str>, ()>) {
//             let lexer = Lexer::new(source);
//             let result = ProgramParser::new()
//                 .parse(source, lexer)
//                 .map_err(|_| ())
//                 .map(|program| program.0)
//                 .map(|exprs| {
//                     exprs
//                         .into_iter()
//                         .map(|node| format!("{:?}", node))
//                         .collect::<Vec<_>>()
//                 });

//             let expect = expect.map(|s| s.into_iter().map(ToString::to_string).collect());

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result, expect)
//         }

//         #[test_case("null;null", Ok(vec!["Atom(Null)", "Atom(Null)"]))]
//         #[test_case("123; true", Ok(vec!["Atom(Integer(123))", "Atom(Boolean(true))"]))]
//         #[test_case("123\n\n true; r/foo/;\n\n1.3", Ok(vec!["Atom(Integer(123))", "Atom(Boolean(true))", "Atom(Regex(/foo/))", "Atom(Float(1.3))"]))]
//         fn multiple_exprs(source: &str, expect: Result<Vec<&str>, ()>) {
//             let lexer = Lexer::new(source);
//             let result = ProgramParser::new()
//                 .parse(source, lexer)
//                 .map(|program| program.0)
//                 .map(|exprs| {
//                     exprs
//                         .into_iter()
//                         .map(|node| format!("{:?}", node))
//                         .collect::<Vec<_>>()
//                 });

//             let expect = expect.map(|s| s.into_iter().map(ToString::to_string).collect());

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), expect)
//         }

//         #[test_case(
//             "r/foo/;{ \n 123 \n\n\n true }\nif true { \n\n 0 } else { 0.1 }",
//             Ok(vec![
//                 "Atom(Regex(/foo/))",
//                 "Container(Block(Block([Atom(Integer(123)), Atom(Boolean(true))])))",
//                 "IfStatement(true ? Block([Atom(Integer(0))]) : Block([Atom(Float(0.1))]))"
//             ]))]
//         fn multiline_exprs(source: &str, expect: Result<Vec<&str>, ()>) {
//             let lexer = Lexer::new(source);
//             let result = ProgramParser::new()
//                 .parse(source, lexer)
//                 .map_err(|_| ())
//                 .map(|program| program.0)
//                 .map(|exprs| {
//                     exprs
//                         .into_iter()
//                         .map(|node| format!("{:?}", node))
//                         .collect::<Vec<_>>()
//                 });

//             let expect = expect.map(|s| s.into_iter().map(ToString::to_string).collect());

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result, expect)
//         }

//         #[test_case(".foo .bar", Ok(vec!["<error>"]))]
//         #[test_case("null null", Ok(vec!["<error>"]))]
//         fn multiple_exprs_per_line(source: &str, expect: Result<Vec<&str>, ()>) {
//             let lexer = Lexer::new(source);
//             let result = ProgramParser::new()
//                 .parse(source, lexer)
//                 .map(|program| program.0)
//                 .map(|exprs| {
//                     exprs
//                         .into_iter()
//                         .map(|node| format!("{:?}", node))
//                         .collect::<Vec<_>>()
//                 });

//             let expect = expect.map(|s| s.into_iter().map(ToString::to_string).collect());

//             if result.is_ok() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), expect)
//         }
//     }

//     mod spanned {
//         use super::*;
//         use test_case::test_case;

//         #[test_case("123", (0, 3))]
//         #[test_case("true", (0, 4))]
//         #[test_case("12.345678", (0, 9))]
//         #[test_case("r/foo|bar|baz/", (0, 14))]
//         #[test_case("null", (0, 4))]
//         #[test_case(r#""foobar""#, (0, 8))]
//         fn atom(source: &str, expect: (usize, usize)) {
//             let source = format!("_r {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map_err(|_| ())
//                 .map(Test::expr)
//                 .map(|node| (node.start() - 3, node.end() - 3));

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result, Ok(expect))
//         }

//         #[test_case("[1, 2,   3]", (0, 11), vec![(1,2), (4,5), (9,10)])]
//         #[test_case("[1, 2,\n\n   3,  ]   ", (0, 16), vec![(1,2), (4,5), (11,12)])]
//         fn array(source: &str, all: (usize, usize), each: Vec<(usize, usize)>) {
//             let source = format!("_r {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map(Test::expr)
//                 .map(|node| {
//                     let all = (node.start() - 3, node.end() - 3);

//                     match node.into_inner() {
//                         Expr::Container(v) => {
//                             let each = match v.into_inner() {
//                                 Container::Array(array) => array
//                                     .into_inner()
//                                     .into_iter()
//                                     .map(|node| (node.start() - 3, node.end() - 3))
//                                     .collect::<Vec<_>>(),
//                                 v => panic!(v),
//                             };

//                             (all, each)
//                         }
//                         v => panic!(v),
//                     }
//                 });

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result.map_err(|_| ()), Ok((all, each)))
//         }

//         #[test_case(r#"{"foo":1}"#, (0, 9), vec![(7,8)])]
//         #[test_case("{\"bar\":1,   \"foo\": \"baz\" \n\n  }", (0, 30), vec![(7,8),(19,24)])]
//         fn object(source: &str, all: (usize, usize), each: Vec<(usize, usize)>) {
//             let source = format!("_r {}", source);

//             let lexer = Lexer::new(&source);
//             let result = TestParser::new()
//                 .parse(&source, lexer)
//                 .map_err(|_| ())
//                 .map(Test::expr)
//                 .map(|node| {
//                     let all = (node.start() - 3, node.end() - 3);

//                     match node.into_inner() {
//                         Expr::Container(v) => {
//                             let each = match v.into_inner() {
//                                 Container::Object(v) => v
//                                     .into_inner()
//                                     .into_iter()
//                                     .map(|(_, node)| (node.start() - 3, node.end() - 3))
//                                     .collect::<Vec<_>>(),
//                                 v => panic!(v),
//                             };

//                             (all, each)
//                         }
//                         v => panic!(v),
//                     }
//                 });

//             if result.is_err() {
//                 dbg!(&result);
//             }

//             assert_eq!(result, Ok((all, each)))
//         }
//     }
// }
