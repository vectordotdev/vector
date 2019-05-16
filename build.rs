fn main() {
    prost_build::compile_protos(&["proto/event.proto"], &["proto/"]).unwrap();
    built::write_built_file().unwrap();
}
