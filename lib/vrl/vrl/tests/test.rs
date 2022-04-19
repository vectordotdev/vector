use std::collections::BTreeMap;
use vector_common::TimeZone;
use vrl::Value;

#[test]
fn test() {
    println!("yo!");

    let source = r#"
        upcase("hi")
    "#;

    let tz = TimeZone::default();
    let functions = vrl_stdlib::all();
    let mut external_env = vrl::state::ExternalEnv::default();
    let (program, local_env, _) =
        vrl::compile_with_state(source, &functions, &mut external_env).unwrap();
    let builder = vrl::llvm::Builder::new().unwrap();
    let context = builder
        .compile((&local_env, &external_env), &program)
        .unwrap();
    // context.optimize();
    let execute = context.get_jit_function().unwrap();

    {
        println!("yo");
        let mut obj = Value::Object(BTreeMap::default());
        let mut context = core::Context {
            target: &mut obj,
            timezone: &tz,
        };
        let mut result = Ok(Value::Null);
        println!("bla");
        unsafe { execute.call(&mut context, &mut result) };
        println!("derp");
    }
}
