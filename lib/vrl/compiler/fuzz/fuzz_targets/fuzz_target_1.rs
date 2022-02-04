#![no_main]
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;
use vrl_compiler as vrl;

fn dump(text: &str) {
    use std::io::prelude::*;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open("nork.txt")
        .unwrap();

    file.write_all(text.as_bytes()).unwrap();
    file.write_all(b"\n\n").unwrap();
}

fuzz_target!(|data: Vec<parser::Program>| {
    for expr in data {
        // Compile the VRL.
        let exprstr = format!("{}", expr);
        let exprdebug = format!("{:?}", expr);
        dump(&format!("Compiling {:?}", exprstr));

        match vrl::compile(expr, &vec![]) {
            Ok(program) => {
                dump(&format!("Compiled {:?}", exprstr));
                let timezone = Default::default();
                let mut runtime = core::Runtime::default();

                let mut target_vm = vrl::Value::Object(BTreeMap::new());
                let mut target_resolve = vrl::Value::Object(BTreeMap::new());

                // Run the VRL in the VM
                let vm = runtime.compile(vec![], &program).unwrap();
                let result_vm = runtime.run_vm(&vm, &mut target_vm, &timezone);

                // Resolve the VRL
                let result_resolve = runtime.resolve(&mut target_resolve, &program, &timezone);

                if result_vm != result_resolve {
                    println!(" OOOOOPS result");
                    println!("expr    : {}", exprstr);
                    println!("debug   : {}", exprdebug);
                    println!("vm      : {:?}", result_vm);
                    println!("resolve : {:?}", result_resolve);
                    println!("-------=======------");
                }

                if target_vm != target_resolve {
                    println!(" OOOOOPS target");
                    println!("expr    : {}", exprstr);
                    println!("debug   : {}", exprdebug);
                    println!("vm      : {:?}", target_vm);
                    println!("resolve : {:?}", target_resolve);
                    println!("-------=======------");
                }

                // Ensure the results are the same
                assert_eq!(result_vm, result_resolve);
                assert_eq!(target_vm, target_resolve);
            }
            Err(_) => {
                // Ignore any programs that don't compile.
                continue;
            }
        }
    }
});
