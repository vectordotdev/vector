#![allow(missing_docs)]
use vector_lib::configurable::schema::generate_root_schema;

use crate::config::ConfigBuilder;

pub fn cmd() -> exitcode::ExitCode {
    match generate_root_schema::<ConfigBuilder>() {
        Ok(schema) => {
            let json = serde_json::to_string_pretty(&schema)
                .expect("rendering root schema to JSON should not fail");

            #[allow(clippy::print_stdout)]
            {
                println!("{}", json);
            }
            exitcode::OK
        }
        Err(e) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("error while generating schema: {:?}", e);
            }
            exitcode::SOFTWARE
        }
    }
}
