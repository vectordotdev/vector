fn main() {
    let mut config = prost_build::Config::new();
    config.type_attribute("message.AddressBook",
                          "#[derive(Serialize, Deserialize)]");
    config.type_attribute("message.Person",
                          "#[derive(Serialize, Deserialize)]");
    config.compile_protos(&["src/message.proto"],
                                &["src/"]).unwrap();
}
