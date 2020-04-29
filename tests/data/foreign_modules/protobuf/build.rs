fn main() {
    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config
        .compile_protos(&["src/message.proto"], &["src/"])
        .unwrap();
}
