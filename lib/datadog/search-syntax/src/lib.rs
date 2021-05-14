mod ast;

use vrl_compiler::compile;

#[macro_use]
extern crate lalrpop_util;

lalrpop_mod!(pub grammar);

#[test]
fn show() {
    let res = grammar::ExprParser::new()
        .parse(r#"@test.tag:hello"#)
        .unwrap();

    let funcs = vec![];
    let program = compile(res.to_vrl(), &funcs).unwrap();

    for exp in program.into_iter() {
        println!("{}", exp);
    }
}
