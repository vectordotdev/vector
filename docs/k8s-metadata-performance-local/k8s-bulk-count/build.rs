use std::{env, fs, path::PathBuf};
fn main() {
    let source = "/Users/al03160720/oss/vector/src/sources/kubernetes_logs/pod_metadata_annotator.rs";
    println!("cargo:rerun-if-changed={source}");
    let text = fs::read_to_string(source).unwrap();
    let helper = text.split("fn merge_metadata_map").nth(1).unwrap()
        .split("fn annotate_from_pod_spec").next().unwrap();
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("helper.rs"), format!("fn merge_metadata_map{helper}")).unwrap();
}
