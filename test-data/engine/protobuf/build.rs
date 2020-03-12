fn main() {
    prost_build::compile_protos(&["src/message.proto"],
                                &["src/"]).unwrap();
}
