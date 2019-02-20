fn main() {
    prost_build::compile_protos(&["src/record.proto"], &["src/"]).unwrap();
}
