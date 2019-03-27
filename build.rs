fn main() {
    prost_build::compile_protos(&["proto/record.proto"], &["proto/"]).unwrap();
}
