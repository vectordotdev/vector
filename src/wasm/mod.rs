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
use std::collections::LinkedList;
use std::{fmt::Debug, fs, path::Path};
use vector_wasm::{Registration, Role, WasmModuleConfig};
mod artifact_cache;
mod fingerprint;
pub use artifact_cache::ArtifactCache;
pub use fingerprint::Fingerprint;

mod context;

pub mod hostcall; // Pub is required for lucet.

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

        let internal_event_compilation =
            internal_events::WasmCompilationProgress::begin(config.role);
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
            error!("Not registered! Please fill your `init` call with a `Registration::transform().register()`.");
        }

        Ok(wasm_module)
    }

    pub fn process(&mut self, mut data: Event) -> Result<LinkedList<Event>> {
        let internal_event_processing = internal_events::EventProcessingProgress::begin(self.role);

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

        let retval = self.instance.run(
            "process",
            &[
                (guest_data_ptr as u32).into(),
                (guest_data_size as u32).into(),
            ],
        );

        let _ = self
            .instance
            .run(
                "drop_buffer",
                &[
                    (guest_data_ptr as u32).into(),
                    (guest_data_size as u32).into(),
                ],
            )?
            .returned()?;

        match retval {
            Ok(_num_events) => {
                let context::EventBuffer { events: out } = self
                    .instance
                    .remove_embed_ctx()
                    .ok_or("Could not retrieve context after processing.")?;

                if let Some(context::RaisedError { error: Some(error) }) =
                    self.instance.remove_embed_ctx()
                {
                    internal_event_processing.error(error);
                } else {
                    internal_event_processing.complete()
                }
                Ok(out)
            }
            Err(lucet_runtime::Error::RuntimeFault(fault)) => {
                let error = format!("WASM instance faulted, resetting: {:?}", fault);
                internal_event_processing.error(error);
                self.instance.reset()?;
                Ok(Default::default())
            }
            Err(e) => {
                internal_event_processing.error(format!("{:?}", e));
                Ok(Default::default())
            }
        }
    }

    pub fn shutdown(&mut self) -> Result<()> {
        let _worked = self.instance.run("shutdown", &[])?;
        Ok(())
    }
}

#[test]
fn protobuf() -> Result<()> {
    use serde_json::json;
    use std::{
        collections::HashMap,
        io::{Read, Write},
    };

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
        "tests/data/wasm/add_fields/target/wasm32-wasi/release/protobuf.wasm",
        "target/artifacts",
        HashMap::new(),
        16 * 64 * 1024 * 10, // 10MB
    ))?;
    let out = module.process(event.clone())?;
    module.shutdown()?;

    let retval = out.into_iter().next().unwrap();
    assert_eq!(
        serde_json::to_value(retval.as_log().get("processed").unwrap()).unwrap(),
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
