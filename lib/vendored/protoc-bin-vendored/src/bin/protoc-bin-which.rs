use std::process;

fn main() {
    let protoc_bin_path = match protoc_bin_vendored::protoc_bin_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("protoc binary not found: {}", e);
            process::exit(1);
        }
    };
    println!("{}", protoc_bin_path.display());
}
