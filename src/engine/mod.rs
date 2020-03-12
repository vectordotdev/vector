//! The WebAssembly Execution Engine
//!
//! This module contains the Vector transparent WebAssembly Engine.

// TODO: FreeBSD: https://github.com/bytecodealliance/lucet/pull/419

use crate::{Error, Event, Result};
use lru::LruCache;
use lucet_runtime::c_api::*;
use lucet_runtime::{
    DlModule, Instance, InstanceBuilder, InstanceHandle, Limits, MmapRegion, Region,
};
use lucet_wasi::WasiCtxBuilder;
use lucetc::{HeapSettings, Bindings};
use lucetc::{Lucetc, LucetcOpts};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

mod util;
mod context;
use context::EngineContext;
use crate::topology::unit_test::build_unit_tests;

pub mod hostcall; // Pub is required for lucet.
mod defaults {
    use std::path::Path;

    pub(super) const BUILDER_CACHE_SIZE: usize = 50;
    pub(super) const ARTIFACT_CACHE: &str = "cache";
}

trait Engine {
    fn build(config: EngineConfig) -> Self;
    fn load<P>(&mut self, path: P) -> Result<()>
    where
        P: Into<PathBuf>;
    fn instantiate<P>(&mut self, path: P) -> Result<Uuid>
    where
        P: Into<PathBuf>;
    fn process(&mut self, id: &Uuid, events: Event) -> Result<Option<Event>>;
}

#[derive(Derivative, Clone)]
#[derivative(Default)]
struct EngineConfig {
    /// Since the engine may load or unload instances over the course of it's life, it uses an LRU
    /// cache to maintain instance builders.
    #[derivative(Default(value = "defaults::BUILDER_CACHE_SIZE"))]
    builder_cache_size: usize,
    #[derivative(Default(value = "defaults::ARTIFACT_CACHE.into()"))]
    artifact_cache: PathBuf,
}

fn compile(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<()> {
    let mut bindings = lucet_wasi::bindings();
    bindings.extend(&Bindings::from_str(include_str!("hostcall/bindings.json"))?)?;
    Ok(Lucetc::new(input)
        .with_bindings(bindings)
        .shared_object_file(output)?)
}

struct DefaultEngine {
    /// A stored version of the config for later referenciing.
    config: EngineConfig,
    /// Currently cached instance builders.
    modules: LruCache<PathBuf, Arc<DlModule>>,
    /// Handles for instantiated instances.
    instance_handles: BTreeMap<Uuid, InstanceHandle>,
}

impl Engine for DefaultEngine {
    fn build(config: EngineConfig) -> Self {
        lucet_wasi::export_wasi_funcs();
        Self {
            config: config.clone(),
            modules: LruCache::new(config.builder_cache_size),
            instance_handles: Default::default(),
        }
    }

    fn load<P>(&mut self, path: P) -> Result<()>
    where
        P: Into<PathBuf>,
    {
        let path = path.into();
        let output_file = self
            .config
            .artifact_cache
            .join(path.file_stem().ok_or("Must load files")?)
            .with_extension("so");

        fs::create_dir_all(&self.config.artifact_cache)?;
        compile(&path, &output_file)?;
        // load the compiled Lucet module
        let dl_module = DlModule::load(&output_file).unwrap();
        self.modules.put(path, dl_module);
        Ok(())
    }

    fn instantiate<P>(&mut self, path: P) -> Result<Uuid>
    where
        P: Into<PathBuf>,
    {
        let path = path.into();
        let module = self.modules.get(&path).ok_or("Could not load path")?;
        // create a new memory region with default limits on heap and stack size
        let region = &MmapRegion::create(1, &Limits {
            heap_memory_size: 16 * 64 * 1024 * 10, // 10MB
            ..Limits::default()
        })?;
        // instantiate the module in the memory region
        let instance = region.new_instance_builder(module.clone()).build()?;

        let id = uuid::Uuid::new_v4();
        self.instance_handles.insert(id.clone(), instance);
        Ok(id)
    }

    fn process(&mut self, id: &Uuid, event: Event) -> Result<Option<Event>> {
        let instance = self
            .instance_handles
            .get_mut(id)
            .ok_or("Could not load instance")?;

        // The instance context is essentially an anymap, so this these aren't colliding!
        let wasi_ctx = WasiCtxBuilder::new().build()?;
        instance.insert_embed_ctx(wasi_ctx);
        let engine_context = EngineContext::new(event);
        instance.insert_embed_ctx(engine_context);

        let worked = instance.run("process", &[])?;

        let engine_context: EngineContext = instance.remove_embed_ctx()
            .ok_or("Could not retrieve context after processing.")?;
        let EngineContext { event: out } = engine_context;
        Ok(out)
    }
}

#[test]
fn inspect() -> Result<()> {
    let module = "target/wasm32-wasi/release/inspect.wasm";
    let mut engine = DefaultEngine::build(Default::default());
    let event = Event::new_empty_log();

    engine.load(module)?;
    let id = engine.instantiate(module)?;
    let out = engine.process(&id, event.clone())?;
    assert_eq!(event, out.unwrap());
    Ok(())
}

#[test]
fn protobuf() -> Result<()> {
    let module = "target/wasm32-wasi/release/protobuf.wasm";
    let mut engine = DefaultEngine::build(Default::default());
    let mut event = Event::new_empty_log();
    event.as_mut_log().insert("test", "testing");

    engine.load(module)?;
    let id = engine.instantiate(module)?;
    let out = engine.process(&id, event.clone())?;
    println!("{:#?}", out);
    Ok(())
}

// #[test]
// fn tester() {
//     lucet_wasi::export_wasi_funcs();
//     // let bindings = lucetc::Bindings::empty();
//     lucetc::Lucetc::new("untitled.wasm")
//         .with_bindings(lucet_wasi::bindings())
//         .shared_object_file("untitled.so")
//         .unwrap();
//     // ensure the WASI symbols are exported from the final executable
//     // load the compiled Lucet module
//     let dl_module = DlModule::load("untitled.so").unwrap();
//     // create a new memory region with default limits on heap and stack size
//     let region = MmapRegion::create(1, &Limits::default()).unwrap();
//     // instantiate the module in the memory region
//     let mut instance_builder = region.new_instance_builder(dl_module);
//     let mut instance = instance_builder.build().unwrap();
//     // prepare the WASI context, inheriting stdio handles from the host executable
//     // let wasi_ctx = WasiCtxBuilder::new().inherit_stdio().build().unwrap();
//     // instance.insert_embed_ctx(wasi_ctx);
//     // run the WASI main function
//     instance.run("test", &[]).unwrap();
// }
//
