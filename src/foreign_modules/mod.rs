//! Foreign module support
//!
//! This module contains the implementation code of our foreign module support. The core traits of
//! our foreign module support exist in the `foreign_modules` crate.
//!
//! **Note:** This code is experimental.
//!

use crate::{Event, Result};
use lucet_runtime::{DlModule, InstanceHandle, Limits, MmapRegion, Region};
use lucet_wasi::WasiCtxBuilder;
use lucetc::Bindings;
use lucetc::{Lucetc, LucetcOpts};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{instrument, Level};

mod context;
mod util;
use context::ForeignModuleContext;
use std::fmt::Debug;
use std::marker::PhantomData;

pub mod hostcall; // Pub is required for lucet.
mod defaults {
    pub(super) const ARTIFACT_CACHE: &str = "cache";
    pub(super) const HEAP_MEMORY_SIZE: usize = 16 * 64 * 1024 * 10; // 10MB
}

/// The base configuration required for a WasmModule.
///
/// If you're designing a module around the WasmModule type, you need to build it with one of these.
#[derive(Derivative, Clone, Debug)]
#[derivative(Default)]
pub struct WasmModuleConfig {
    path: PathBuf,
    #[derivative(Default(value = "defaults::ARTIFACT_CACHE.into()"))]
    artifact_cache: PathBuf,
    #[derivative(Default(value = "defaults::HEAP_MEMORY_SIZE"))]
    heap_memory_size: usize,
}

impl WasmModuleConfig {
    pub fn new(path: impl Into<PathBuf>, artifact_cache: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            artifact_cache: artifact_cache.into(),
            heap_memory_size: defaults::HEAP_MEMORY_SIZE,
        }
    }

    pub fn set_heap_memory_size(&mut self, heap_memory_size: usize) -> &mut Self {
        self.heap_memory_size = heap_memory_size;
        self
    }
}

/// Compiles a WASM module located at `input` and writes an optimized shared object to `output`.
#[instrument]
fn compile(input: impl AsRef<Path> + Debug, output: impl AsRef<Path> + Debug) -> Result<()> {
    event!(Level::INFO, "begin");

    let mut bindings = lucet_wasi::bindings();
    bindings.extend(&Bindings::from_str(include_str!("hostcall/bindings.json"))?)?;
    let ret = Lucetc::new(input)
        .with_bindings(bindings)
        .shared_object_file(output)?;

    event!(Level::INFO, "done");
    Ok(ret)
}

/// A foreign module that is operating as a WASM guest.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct WasmModule<Archetype> {
    /// A stored version of the config for later referencing.
    config: WasmModuleConfig,
    /// The handle to the Lucet instance.
    #[derivative(Debug="ignore")]
    instance: InstanceHandle,
    archetype: PhantomData<Archetype>,
}

impl<Archetype> WasmModule<Archetype> {
    /// Build the WASM instance from a given config.
    #[instrument]
    pub fn build(config: impl Into<WasmModuleConfig> + Debug) -> Result<Self> {
        event!(Level::TRACE, "instantiating");
        let config = config.into();
        let output_file = config
            .artifact_cache
            .join(config.path.file_stem().ok_or("Must load files")?)
            .with_extension("so");

        fs::create_dir_all(&config.artifact_cache)?;
        compile(&config.path, &output_file)?;
        // load the compiled Lucet module
        let module = DlModule::load(&output_file).unwrap();

        // create a new memory region with default limits on heap and stack size
        let region = &MmapRegion::create(
            1,
            &Limits {
                heap_memory_size: config.heap_memory_size,
                ..Limits::default()
            },
        )?;
        // instantiate the module in the memory region
        let instance = region.new_instance_builder(module).build()?;

        let wasm_module = Self {
            config,
            instance,
            archetype: Default::default(),
        };
        event!(Level::TRACE, "instantiated");
        Ok(wasm_module)
    }
}


impl WasmModule<foreign_modules::roles::Transform> {
    #[instrument]
    pub fn process(&mut self, event: Event) -> Result<Option<Event>> {
        event!(Level::TRACE, "processing");

        // The instance context is essentially an anymap, so this these aren't colliding!
        let wasi_ctx = WasiCtxBuilder::new().inherit_stdio().build()?;
        self.instance.insert_embed_ctx(wasi_ctx);

        let engine_context = ForeignModuleContext::new(event);
        self.instance.insert_embed_ctx(engine_context);

        let _worked = self.instance.run("process", &[])?;

        let engine_context: ForeignModuleContext = self.instance
            .remove_embed_ctx()
            .ok_or("Could not retrieve context after processing.")?;
        let ForeignModuleContext { event: out } = engine_context;

        event!(Level::TRACE, "processed");
        Ok(out)
    }
}

#[test]
fn protobuf() -> Result<()> {
    use std::io::{Read, Write};
    use string_cache::DefaultAtom as Atom;
    crate::test_util::trace_init();

    // Load in fixtures.
    let mut test_file = fs::File::open("test-data/foreign_modules/protobuf/demo.pb")?;
    let mut buf = String::new();
    test_file.read_to_string(&mut buf)?;
    let mut event = Event::new_empty_log();
    event.as_mut_log().insert("test", buf);

    // Refresh the test json.
    let event_string = serde_json::to_string(&event.as_log())?;
    let mut json_fixture = fs::File::create("test-data/foreign_modules/protobuf/demo.json")?;
    json_fixture.write(event_string.as_bytes());

    // Run the test.
    let mut module = WasmModule::build(WasmModuleConfig::new("target/wasm32-wasi/release/protobuf.wasm", "cache", ))?;
    let out = module.process(event.clone())?;

    let retval = out.unwrap();
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
