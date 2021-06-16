extern crate lalrpop;

use std::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::{env, fs};

fn main() {
    lalrpop::Configuration::new()
        .always_use_colors()
        .process_current_dir()
        .unwrap();

    println!("cargo:rerun-if-changed=src/grok.lalrpop");

    read_grok_patterns();
}

/// Reads grok patterns defined in the `patterns` folder into the static `PATTERNS` variable
fn read_grok_patterns() {
    let mut output = String::new();

    fmt::write(
        &mut output,
        format_args!("static PATTERNS: &[(&str, &str)] = &[\n"),
    )
    .unwrap();

    fs::read_dir(Path::new("patterns"))
        .unwrap()
        .map(|e| e.unwrap())
        .map(|path| File::open(path.path()).unwrap())
        .flat_map(|f| BufReader::new(f).lines())
        .map(|line| line.unwrap())
        .filter(|line| !line.starts_with("#"))
        .filter(|line| !line.is_empty())
        .for_each(|line| {
            let (key, value) = line.split_at(line.find(" ").unwrap());
            fmt::write(
                &mut output,
                format_args!("\t(\"{}\", r#\"{}\"#),\n", key, &value[1..]),
            )
            .unwrap();
        });

    fmt::write(&mut output, format_args!("];\n")).unwrap();

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("patterns.rs");
    let mut file = File::create(&dest_path).unwrap();
    file.write_all(output.as_bytes()).unwrap();
}
