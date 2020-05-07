//! Foreign module support
//!
//! This module contains the implementation code of our foreign module support. The core traits of
//! our foreign module support exist in the `vector-wasm` crate.
//!
//! **Note:** This code is experimental.

use crate::{internal_events, Event, Result};
use context::EventBuffer;
use lucet_runtime::{DlModule, InstanceHandle, Limits, MmapRegion, Region};
use lucet_wasi::WasiCtxBuilder;
use lucetc::{Bindings, Lucetc, LucetcOpts};
use serde::{Deserialize, Serialize};
use std::collections::LinkedList;
use std::{
    collections::HashMap,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};
use tracing::Level;
use vector_wasm::{Registration, Role};

mod context;

pub mod hostcall; // Pub is required for lucet.

pub mod defaults {
    pub(super) const ARTIFACT_CACHE: &str = "cache";
    pub(super) const HEAP_MEMORY_SIZE: usize = 16 * 64 * 1024 * 10; // 10MB
}

/// The base configuration required for a WasmModule.
///
/// If you're designing a module around the WasmModule type, you need to build it with one of these.
#[derive(Derivative, Clone, Debug, Deserialize, Serialize)]
#[derivative(Default)]
pub struct WasmModuleConfig {
    /// The role which the module will play.
    #[derivative(Default(value = "Role::Transform"))]
    pub role: Role,
    /// The path to the module's `wasm` file.
    pub path: PathBuf,
    /// The cache location where an optimized `so` file shall be placed.
    ///
    /// This folder also stores a `.fingerprints` file that is formatted as a JSON map, matching file paths
    /// to fingerprints.
    #[derivative(Default(value = "defaults::ARTIFACT_CACHE.into()"))]
    pub artifact_cache: PathBuf,
    /// The maximum size of the heap the module may grow to.
    // TODO: The module may also declare it's minimum heap size, and they will be compared before
    //       the module begins processing.
    #[derivative(Default(value = "defaults::HEAP_MEMORY_SIZE"))]
    pub max_heap_memory_size: usize,
}

impl WasmModuleConfig {
    /// Build a new configuration with the required options set.
    pub fn new(role: Role, path: impl Into<PathBuf>) -> Self {
        Self {
            role,
            path: path.into(),
            artifact_cache: defaults::ARTIFACT_CACHE.into(),
            max_heap_memory_size: defaults::HEAP_MEMORY_SIZE,
        }
    }

    /// Set the maximum heap size of the transform to the given value. See `defaults::HEAP_MEMORY_SIZE`.
    pub fn set_max_heap_memory_size(&mut self, max_heap_memory_size: usize) -> &mut Self {
        self.max_heap_memory_size = max_heap_memory_size;
        self
    }

    /// Set the maximum heap size of the transform to the given value. See `defaults::HEAP_MEMORY_SIZE`.
    pub fn set_artifact_cache(&mut self, artifact_cache: impl Into<PathBuf>) -> &mut Self {
        self.artifact_cache = artifact_cache.into();
        self
    }
}

// TODO: Make a proper fingerprinted artifact type.

type Fingerprint = u64;

/// Get the age (since UNIX epoch) of a file.
fn calculate_fingerprint(file: impl AsRef<Path> + Debug) -> Result<Fingerprint> {
    let path = file.as_ref();
    let meta = std::fs::metadata(path)?;

    let modified = meta.modified()?;
    let age = modified.duration_since(std::time::UNIX_EPOCH)?;
    Ok(age.as_secs())
}

/// Returns the stored JSON map in the fingerprint store.
///
/// If the `$ARTIFACT_CACHE/.fingerprints` file does not exist, we return an empty HashMap.
fn fetch_known_fingerprints(
    artifact_cache: impl AsRef<Path> + Debug,
) -> Result<HashMap<PathBuf, Fingerprint>> {
    let artifact_cache = artifact_cache.as_ref();
    let mut fingerprint_pathbuf = artifact_cache.to_path_buf();
    fingerprint_pathbuf.push(".fingerprints");

    match std::fs::File::open(fingerprint_pathbuf) {
        Ok(fingerprint_file) => {
            let reader = std::io::BufReader::new(fingerprint_file);
            let fingerprints = serde_json::from_reader(reader)?;
            Ok(fingerprints)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(Box::new(e)),
    }
}

/// Parse `$ARTIFACT_CACHE/.fingerprints` and determine the fingerprint of the file path (which should be the wasm file).
fn fetch_fingerprint(
    artifact_cache: impl AsRef<Path> + Debug,
    file: impl AsRef<Path> + Debug,
) -> Result<Option<Fingerprint>> {
    let artifact_cache = artifact_cache.as_ref();
    let file_path = file.as_ref();

    let fingerprints = fetch_known_fingerprints(artifact_cache)?;
    Ok(fingerprints.get(file_path).cloned())
}

/// Parse `$ARTIFACT_CACHE/.fingerprints` and add the given fingerprint.
fn store_fingerprint(
    artifact_cache: impl AsRef<Path> + Debug,
    file: impl AsRef<Path> + Debug,
    fingerprint: Fingerprint,
) -> Result<()> {
    let artifact_cache = artifact_cache.as_ref();
    let file_path = file.as_ref();

    let mut fingerprints = fetch_known_fingerprints(artifact_cache)?;
    fingerprints.insert(file_path.to_path_buf(), fingerprint);

    let mut fingerprint_pathbuf = artifact_cache.to_path_buf();
    fingerprint_pathbuf.push(".fingerprints");

    let fingerprint_file = std::fs::File::create(fingerprint_pathbuf)?;
    let writer = std::io::BufWriter::new(fingerprint_file);
    serde_json::to_writer_pretty(writer, &fingerprints)?;
    Ok(())
}

/// Compiles a WASM module located at `input` and writes an optimized shared object to `output`.
fn compile(
    input: impl AsRef<Path> + Debug,
    output: impl AsRef<Path> + Debug,
) -> Result<Fingerprint> {
    let input = input.as_ref();
    let fingerprint = calculate_fingerprint(input)?;

    let mut bindings = lucet_wasi::bindings();
    bindings.extend(&Bindings::from_str(include_str!("hostcall/bindings.json"))?)?;
    Lucetc::new(input)
        .with_bindings(bindings)
        .shared_object_file(output)?;

    event!(Level::INFO, "done");
    Ok(fingerprint)
}

/// A foreign module that is operating as a WASM guest.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct WasmModule {
    /// A stored version of the config for later referencing.
    config: WasmModuleConfig,
    /// The handle to the Lucet instance.
    #[derivative(Debug = "ignore")]
    instance: InstanceHandle,
    role: Role,
}

impl WasmModule {
    /// Build the WASM instance from a given config.
    pub fn build(config: impl Into<WasmModuleConfig> + Debug) -> Result<Self> {
        event!(Level::TRACE, "instantiating");
        let config = config.into();
        let output_file = config
            .artifact_cache
            .join(config.path.file_stem().ok_or("Must load files")?)
            .with_extension("so");

        // Prepwork
        fs::create_dir_all(&config.artifact_cache)?;
        hostcall::ensure_linked();

        let calculated_fingerprint = calculate_fingerprint(&config.path)?;
        let found_fingerprint = fetch_fingerprint(&config.artifact_cache, &config.path)?;
        match found_fingerprint {
            Some(found) if found == calculated_fingerprint => {
                // We can be lazy and do nothing! How wonderful.
                // println!("Skipped wasm compile! Make me a metric later!");
            }
            None | Some(_) => {
                // println!("Rebuild wasm! Make me a metric later!");
                let internal_event_compilation =
                    internal_events::WasmCompilation::begin(config.role);
                let fingerprint = compile(&config.path, &output_file)?;
                store_fingerprint(&config.artifact_cache, &config.path, fingerprint)?;
                internal_event_compilation.complete();
            }
        }

        // load the compiled Lucet module
        let module = DlModule::load(&output_file).unwrap();

        // create a new memory region with default limits on heap and stack size
        let region = &MmapRegion::create(
            1,
            &Limits {
                heap_memory_size: config.max_heap_memory_size,
                ..Limits::default()
            },
        )?;

        // instantiate the module in the memory region
        let instance = region
            .new_instance_builder(module)
            .with_embed_ctx::<Option<Registration>>(None)
            .build()?;
        let mut wasm_module = Self {
            config,
            instance,
            role: Role::Transform,
        };

        let wasi_ctx = WasiCtxBuilder::new().inherit_stdio().build()?;
        wasm_module.instance.insert_embed_ctx(wasi_ctx);

        wasm_module.instance.run("init", &[])?.returned()?;
        let registration = wasm_module
            .instance
            .get_embed_ctx::<Option<Registration>>()
            .unwrap()
            .unwrap()
            .clone();

        if let None = registration {
            error!("Unable to find registration");
        }

        Ok(wasm_module)
    }

    pub fn process(&mut self, mut data: Event) -> Result<LinkedList<Event>> {
        let internal_event_processing = internal_events::EventProcessing::begin(self.role);

        self.instance.insert_embed_ctx(EventBuffer::new());

        // We unfortunately can't pass our `Event` type over FFI.
        // Copy 1
        let data_buf = serde_json::to_vec(data.as_mut_log())?;
        let guest_data_size = data_buf.len() as u64;
        let guest_data_ptr = self
            .instance
            .run("allocate_buffer", &[guest_data_size.into()])?
            .returned()?
            .as_u64();
        let guest_data_buf: &mut [u8] = self.instance.heap_mut()
            [guest_data_ptr as usize..(guest_data_ptr as usize + guest_data_size as usize)]
            .as_mut();
        // Copy 2
        guest_data_buf.copy_from_slice(&data_buf);

        let _worked = self
            .instance
            .run("process", &[guest_data_ptr.into(), guest_data_size.into()])?;

        let engine_context: EventBuffer = self
            .instance
            .remove_embed_ctx()
            .ok_or("Could not retrieve context after processing.")?;
        let EventBuffer { events: out } = engine_context;

        internal_event_processing.complete();
        Ok(out)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        event!(Level::TRACE, "shutting down");

        let _worked = self.instance.run("shutdown", &[])?;

        event!(Level::TRACE, "processed");
        Ok(())
    }
}

#[test]
fn protobuf() -> Result<()> {
    use std::io::{Read, Write};
    use string_cache::DefaultAtom as Atom;
    crate::test_util::trace_init();

    // Load in fixtures.
    let mut test_file = fs::File::open("tests/data/wasm/protobuf/demo.pb")?;
    let mut buf = String::new();
    test_file.read_to_string(&mut buf)?;
    let mut event = Event::new_empty_log();
    event.as_mut_log().insert("message", buf);

    // Refresh the test json.
    let event_string = serde_json::to_string(&event.as_log())?;
    let mut json_fixture = fs::File::create("tests/data/wasm/protobuf/demo.json")?;
    json_fixture.write(event_string.as_bytes())?;

    // Run the test.
    let mut module = WasmModule::build(WasmModuleConfig::new(
        Role::Transform,
        "tests/data/wasm/protobuf/protobuf.wat",
    ))?;
    let out = module.process(event.clone())?;
    module.shutdown()?;

    let retval = out.into_iter().next().unwrap();
    assert_eq!(
        retval
            .as_log()
            .get(&Atom::from("processed"))
            .unwrap()
            .to_string_lossy(),
        "{\"people\":[{\"name\":\"Foo\",\"id\":1,\"email\":\"foo@test.com\",\"phones\":[]}]}"
    );

    Ok(())
}
