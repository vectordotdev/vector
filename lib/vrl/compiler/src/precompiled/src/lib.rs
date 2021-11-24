pub use vrl_compiler::{Context, Value};

pub fn foo() -> u32 {
    1337
}

pub fn bar() -> u32 {
    42
}

pub fn baz() -> u32 {
    9000
}

pub fn get_path<'a>(ctx: &mut Context<'a>, path: lookup::LookupBuf) {
    ctx.target().get(&path);
}

// OpCode::GetPath => {
//     let variable = self.next_primitive();
//     let variable = &self.targets[variable];

//     match &variable {
//         Variable::External(path) => {
//             let value = ctx.target().get(path)?.unwrap_or(Value::Null);
//             self.stack.push(value);
//         }
//         Variable::Internal => unimplemented!("variables are junk"),
//     }
// }

// #[no_mangle]
// pub extern "C" fn execute<'a>(ctx: &mut Context<'a>) -> Result<Value, String> {
//     todo!()
// }

extern "Rust" {
    pub fn execute<'a>(ctx: &mut Context<'a>) -> Result<Value, String>;
}
