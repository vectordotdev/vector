fn main() {
    prost_build::compile_protos(&["proto/record.proto"], &["proto/"]).unwrap();
    built::write_built_file().unwrap();
}
