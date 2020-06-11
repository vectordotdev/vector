//! WASM Plugin Support
//!
//! This module contains the implementation code of our plugin module support. The core traits of
//! our plugin support exist in the `vector-wasm` crate.
//!
//! **Note:** This code is experimental.

use crate::{internal_events, Event, Result};
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
use vector_wasm::{Registration, Role};
mod artifact_cache;
mod fingerprint;
pub use artifact_cache::ArtifactCache;
pub use fingerprint::Fingerprint;

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
    pub options: HashMap<String, serde_json::Value>,
}

impl WasmModuleConfig {
    /// Build a new configuration with the required options set.
    pub fn new(
        role: Role,
        path: impl Into<PathBuf>,
        artifact_cache: impl Into<PathBuf>,
        options: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            role,
            path: path.into(),
            artifact_cache: artifact_cache.into(),
            // The rest should be configured via setters below...
            max_heap_memory_size: defaults::HEAP_MEMORY_SIZE,
            options,
        }
    }

    /// Set the maximum heap size of the transform to the given value. See `defaults::HEAP_MEMORY_SIZE`.
    pub fn set_max_heap_memory_size(&mut self, max_heap_memory_size: usize) -> &mut Self {
        self.max_heap_memory_size = max_heap_memory_size;
        self
    }
}

/// Compiles a WASM module located at `input` and writes an optimized shared object to `output`.
fn compile(
    input: impl AsRef<Path> + Debug,
    output: impl AsRef<Path> + Debug,
) -> Result<Fingerprint> {
    let input = input.as_ref();
    let fingerprint = Fingerprint::new(input)?;

    let mut bindings = Bindings::empty();
    bindings.extend(&lucet_wasi::bindings())?;
    bindings.extend(&Bindings::env(
        hostcall::HOSTCALL_LIST
            .iter()
            .cloned()
            .map(|f| (String::from(f), String::from(f)))
            .collect(),
    ))?;

    Lucetc::new(input)
        .with_bindings(bindings)
        .shared_object_file(output)?;

    Ok(fingerprint)
}

/// A plugin module that is operating as a WASM guest.
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
        let config = config.into();
        let output_file = config
            .artifact_cache
            .join(config.path.file_stem().ok_or("A file is required")?)
            .with_extension("so");

        // Prepwork
        fs::create_dir_all(&config.artifact_cache)?;
        hostcall::ensure_linked();

        let artifact_cache = ArtifactCache::new(config.artifact_cache.clone())?;

        let internal_event_compilation = internal_events::WasmCompilation::begin(config.role);
        if artifact_cache.has_fresh(&config.path)? {
            // We can be lazy and do nothing! How wonderful.
            internal_event_compilation.cached();
        } else {
            let fingerprint = compile(&config.path, &output_file)?;
            let mut artifact_cache = artifact_cache; // Just for this scope.
            artifact_cache.upsert(&config.path, fingerprint)?;
            internal_event_compilation.complete();
        }

        // load the compiled Lucet module
        let module = DlModule::load(&output_file)?;

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
            .with_embed_ctx::<WasmModuleConfig>(config.clone())
            .with_embed_ctx::<Option<Registration>>(None)
            .with_embed_ctx::<context::RaisedError>(Default::default())
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
            .remove_embed_ctx::<Option<Registration>>()
            .and_then(|v| v);

        if let None = registration {
            error!("Not registered! Please fill your `init` call with a `Registration::transform().register()`!");
        }

        Ok(wasm_module)
    }

    pub fn process(&mut self, mut data: Event) -> Result<LinkedList<Event>> {
        let internal_event_processing = internal_events::EventProcessing::begin(self.role);

        self.instance.insert_embed_ctx(context::EventBuffer::new());
        self.instance
            .insert_embed_ctx::<context::RaisedError>(Default::default());

        // We unfortunately can't pass our `Event` type easily over FFI.
        // This can definitely be improved later with some `Event` type changes.
        let data_buf = serde_json::to_vec(data.as_mut_log())?;
        let guest_data_size = data_buf.len();
        let guest_data_ptr = self
            .instance
            .run("allocate_buffer", &[(guest_data_size as u32).into()])?
            .returned()?
            .as_u32();
        let guest_data_buf: &mut [u8] = self.instance.heap_mut()
            [guest_data_ptr as usize..(guest_data_ptr as usize + guest_data_size as usize)]
            .as_mut();
        guest_data_buf.copy_from_slice(&data_buf);

        match self.instance.run(
            "process",
            &[
                (guest_data_ptr as u32).into(),
                (guest_data_size as u32).into(),
            ],
        ) {
            Ok(_num_events) => (),
            Err(lucet_runtime::Error::RuntimeFault(fault)) => {
                error!(
                    "WASM instance faulted, resetting: {:?}",
                    fault.clone().rip_addr_details.and_then(|v| v.file_name),
                );
                self.instance.reset()?;
            }
            Err(e) => error!("WASM processing errored: {:?}", e,),
        }

        let context::EventBuffer { events: out } = self
            .instance
            .remove_embed_ctx()
            .ok_or("Could not retrieve context after processing.")?;

        if let Some(context::RaisedError { error: Some(error) }) = self.instance.remove_embed_ctx()
        {
            error!("WASM plugin errored: {}", error);
        };

        internal_event_processing.complete();
        Ok(out)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        let _worked = self.instance.run("shutdown", &[])?;
        Ok(())
    }
}

#[test]
fn protobuf() -> Result<()> {
    use serde_json::json;
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
        "target/wasm32-wasi/release/protobuf.wasm",
        "target/artifacts",
        HashMap::new(),
    ))?;
    let out = module.process(event.clone())?;
    module.shutdown()?;

    let retval = out.into_iter().next().unwrap();
    assert_eq!(
        serde_json::to_value(retval.as_log().get(&Atom::from("processed")).unwrap()).unwrap(),
        json!({
            "people": [
                {
                    "name": "Foo",
                    "id": 1,
                    "email": "foo@test.com",
                    "phones": [],
                }
            ],
        }),
    );

    Ok(())
}
