use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/performance_counters.c");

    cc::Build::new()
        .file("src/performance_counters.c")
        .warnings(false)
        .compile("libperformance_counters.a");

    println!("cargo:rustc-link-lib=performance_counters");

    let bindings = bindgen::Builder::default()
        .header("src/performance_counters.c")
        .allowlist_function("init_counting")
        .allowlist_function("print_db_info")
        .allowlist_function("start_counting")
        .allowlist_function("stop_counting")
        .allowlist_function("init_counters")
        .allowlist_function("get_counters")
        .allowlist_function("print_counters")
        .allowlist_function("get_named_counters")
        .allowlist_type("counters")
        .allowlist_type("named_counters")
        .generate()
        .unwrap();

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .unwrap();
}
