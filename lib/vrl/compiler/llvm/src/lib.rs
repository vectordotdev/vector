pub use inkwell::*;

pub mod orc_jit {
    pub use llvm_sys::orc2::{
        LLVMJITSymbolGenericFlags as SymbolGenericFlags, LLVMJITSymbolTargetFlags as TargetFlags,
        LLVMJITSymbolTargetFlags as SymbolTargetFlags,
        LLVMOrcJITDylibLookupFlags as DylibLookupFlags, LLVMOrcLookupKind as LookupKind,
        LLVMOrcSymbolLookupFlags as SymbolLookupFlags,
    };

    use inkwell::{
        context::Context, data_layout::DataLayout, execution_engine::ExecutionEngine,
        module::Module, support::LLVMString,
    };
    use llvm_sys::{
        error::{LLVMCreateStringError, LLVMGetErrorMessage, LLVMOpaqueError},
        orc2::{
            lljit::{
                LLVMOrcCreateLLJIT, LLVMOrcCreateLLJITBuilder, LLVMOrcDisposeLLJIT,
                LLVMOrcDisposeLLJITBuilder, LLVMOrcLLJITAddLLVMIRModule, LLVMOrcLLJITBuilderRef,
                LLVMOrcLLJITBuilderSetJITTargetMachineBuilder, LLVMOrcLLJITGetMainJITDylib,
                LLVMOrcLLJITLookup, LLVMOrcLLJITRef, LLVMOrcOpaqueLLJIT,
            },
            LLVMJITCSymbolMapPair, LLVMJITEvaluatedSymbol, LLVMJITSymbolFlags,
            LLVMOrcAbsoluteSymbols, LLVMOrcCLookupSet, LLVMOrcCreateCustomCAPIDefinitionGenerator,
            LLVMOrcCreateNewThreadSafeContext, LLVMOrcCreateNewThreadSafeModule,
            LLVMOrcDisposeJITTargetMachineBuilder, LLVMOrcDisposeThreadSafeContext,
            LLVMOrcDisposeThreadSafeModule, LLVMOrcJITDylibAddGenerator, LLVMOrcJITDylibDefine,
            LLVMOrcJITDylibRef, LLVMOrcJITTargetMachineBuilderDetectHost,
            LLVMOrcJITTargetMachineBuilderRef, LLVMOrcOpaqueDefinitionGenerator,
            LLVMOrcOpaqueJITDylib, LLVMOrcOpaqueJITTargetMachineBuilder, LLVMOrcOpaqueLookupState,
            LLVMOrcRetainSymbolStringPoolEntry, LLVMOrcSymbolStringPoolEntryStr,
            LLVMOrcThreadSafeContextGetContext, LLVMOrcThreadSafeContextRef,
            LLVMOrcThreadSafeModuleRef,
        },
        prelude::{LLVMContextRef, LLVMModuleRef},
    };
    use std::{
        cell::{Cell, RefCell},
        ffi::{CStr, CString},
        marker::PhantomData,
    };

    pub struct TargetMachineBuilder(LLVMOrcJITTargetMachineBuilderRef);

    impl TargetMachineBuilder {
        pub fn detect_host() -> Result<Self, LLVMString> {
            let mut machine_builder = std::ptr::null_mut::<LLVMOrcOpaqueJITTargetMachineBuilder>();
            let error = unsafe { LLVMOrcJITTargetMachineBuilderDetectHost(&mut machine_builder) };
            if !error.is_null() {
                let error = unsafe { LLVMGetErrorMessage(error) };
                Err(unsafe { std::mem::transmute(error) })
            } else {
                Ok(Self(machine_builder))
            }
        }

        pub fn create_lljit_builder(self) -> LljitBuilder {
            let jit_builder = unsafe { LLVMOrcCreateLLJITBuilder() };
            unsafe { LLVMOrcLLJITBuilderSetJITTargetMachineBuilder(jit_builder, self.0) };
            std::mem::forget(self);
            LljitBuilder(jit_builder)
        }
    }

    impl Drop for TargetMachineBuilder {
        fn drop(&mut self) {
            unsafe { LLVMOrcDisposeJITTargetMachineBuilder(self.0) }
        }
    }

    pub struct LljitBuilder(LLVMOrcLLJITBuilderRef);

    impl LljitBuilder {
        pub fn create_lljit<'a>(self) -> Result<Lljit<'a>, LLVMString> {
            let mut lljit = std::ptr::null_mut::<LLVMOrcOpaqueLLJIT>();
            {
                let error = unsafe { LLVMOrcCreateLLJIT(&mut lljit, self.0) };
                std::mem::forget(self);
                if !error.is_null() {
                    let error = unsafe { LLVMGetErrorMessage(error) };
                    Err(unsafe { std::mem::transmute(error) })
                } else {
                    Ok(Lljit::new(lljit))
                }
            }
        }
    }

    impl Drop for LljitBuilder {
        fn drop(&mut self) {
            unsafe { LLVMOrcDisposeLLJITBuilder(self.0) }
        }
    }

    pub struct Lljit<'a>(LLVMOrcLLJITRef, PhantomData<&'a ()>);

    impl<'a> Lljit<'a> {
        fn new(lljit_ref: LLVMOrcLLJITRef) -> Self {
            Self(lljit_ref, PhantomData)
        }

        pub fn main_jit_dylib(&self) -> Dylib<'a> {
            Dylib::new(unsafe { LLVMOrcLLJITGetMainJITDylib(self.0) })
        }

        pub fn add_module(&self, dylib: Dylib, module: ThreadSafeModule) -> Result<(), LLVMString> {
            let error = unsafe { LLVMOrcLLJITAddLLVMIRModule(self.0, dylib.0, module.0) };
            std::mem::forget(module);
            if !error.is_null() {
                let error = unsafe { LLVMGetErrorMessage(error) };
                Err(unsafe { std::mem::transmute(error) })
            } else {
                Ok(())
            }
        }

        pub fn lookup_function_address(&self, symbol_name: &str) -> Result<usize, LLVMString> {
            let mut function_address = Default::default();
            let symbol_name = CString::new(symbol_name).unwrap();
            let error =
                unsafe { LLVMOrcLLJITLookup(self.0, &mut function_address, symbol_name.as_ptr()) };
            if !error.is_null() {
                let error = unsafe { LLVMGetErrorMessage(error) };
                Err(unsafe { std::mem::transmute(error) })
            } else {
                Ok(function_address as _)
            }
        }
    }

    impl<'a> Drop for Lljit<'a> {
        fn drop(&mut self) {
            let error = unsafe { LLVMOrcDisposeLLJIT(self.0) };
            if !error.is_null() {
                let error = unsafe { LLVMGetErrorMessage(error) };
                let error = unsafe { CStr::from_ptr(error) };
                debug_assert!(false, "Failed disposing LLJIT: {}", error.to_string_lossy());
            }
        }
    }

    pub struct ThreadSafeContext(LLVMOrcThreadSafeContextRef);

    impl ThreadSafeContext {
        pub fn new() -> Self {
            Self(unsafe { LLVMOrcCreateNewThreadSafeContext() })
        }

        pub fn context(&self) -> Context {
            #[allow(dead_code)]
            struct InkwellContext {
                context: LLVMContextRef,
            }

            let context = InkwellContext {
                context: unsafe { LLVMOrcThreadSafeContextGetContext(self.0) },
            };
            unsafe { std::mem::transmute(context) }
        }
    }

    impl Default for ThreadSafeContext {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for ThreadSafeContext {
        fn drop(&mut self) {
            unsafe { LLVMOrcDisposeThreadSafeContext(self.0) }
        }
    }

    #[derive(Clone, Copy)]
    pub struct Dylib<'a>(LLVMOrcJITDylibRef, PhantomData<&'a ()>);

    impl<'a> Dylib<'a> {
        fn new(dylib_ref: LLVMOrcJITDylibRef) -> Self {
            Self(dylib_ref, PhantomData)
        }

        pub fn add_generator<Generator, Context>(&self, context: &mut Context)
        where
            Generator: CustomDefinitionGenerator<Context>,
        {
            let definition_generator = unsafe {
                LLVMOrcCreateCustomCAPIDefinitionGenerator(
                    Generator::definition_generator,
                    context as *mut _ as _,
                )
            };
            unsafe { LLVMOrcJITDylibAddGenerator(self.0, definition_generator) };
        }
    }

    pub struct EvaluatedSymbol {
        pub address: usize,
        pub flags: SymbolFlags,
    }

    impl From<EvaluatedSymbol> for LLVMJITEvaluatedSymbol {
        fn from(symbol: EvaluatedSymbol) -> Self {
            Self {
                Address: symbol.address as _,
                Flags: symbol.flags.into(),
            }
        }
    }

    pub struct SymbolFlags {
        pub generic: u8,
        pub target: u8,
    }

    impl From<SymbolFlags> for LLVMJITSymbolFlags {
        fn from(flags: SymbolFlags) -> Self {
            Self {
                GenericFlags: flags.generic,
                TargetFlags: flags.target,
            }
        }
    }

    trait UnsafeCustomDefinitionGenerator<Context> {
        extern "C" fn definition_generator(
            _definition_generator: *mut LLVMOrcOpaqueDefinitionGenerator,
            context: *mut std::ffi::c_void,
            _lookup_state: *mut *mut LLVMOrcOpaqueLookupState,
            lookup_kind: LookupKind,
            dylib: *mut LLVMOrcOpaqueJITDylib,
            dylib_lookup_flags: DylibLookupFlags,
            lookup_set: LLVMOrcCLookupSet,
            lookup_set_size: usize,
        ) -> *mut LLVMOpaqueError;
    }

    impl<T, Context> UnsafeCustomDefinitionGenerator<Context> for T
    where
        T: CustomDefinitionGenerator<Context>,
    {
        extern "C" fn definition_generator(
            _definition_generator: *mut LLVMOrcOpaqueDefinitionGenerator,
            context: *mut std::ffi::c_void,
            _lookup_state: *mut *mut LLVMOrcOpaqueLookupState,
            lookup_kind: LookupKind,
            dylib: *mut LLVMOrcOpaqueJITDylib,
            dylib_lookup_flags: DylibLookupFlags,
            lookup_set: LLVMOrcCLookupSet,
            lookup_set_size: usize,
        ) -> *mut LLVMOpaqueError {
            let context = unsafe { &mut *(context as *mut Context) };
            let dylib = Dylib::new(dylib);
            for i in 0..lookup_set_size {
                let item = unsafe { &mut *lookup_set.add(i) };
                let symbol_name = unsafe { LLVMOrcSymbolStringPoolEntryStr(item.Name) };
                let symbol_name = unsafe { CStr::from_ptr(symbol_name) };
                let symbol_lookup_flags = &item.LookupFlags;
                let evaluated_symbol = match Self::define_absolute_symbol(
                    context,
                    &lookup_kind,
                    dylib,
                    &dylib_lookup_flags,
                    symbol_name,
                    symbol_lookup_flags,
                ) {
                    Ok(symbol) => symbol,
                    Err(error) => {
                        let error = CString::new(error).unwrap();
                        return unsafe { LLVMCreateStringError(error.as_ptr()) };
                    }
                };
                unsafe { LLVMOrcRetainSymbolStringPoolEntry(item.Name) };
                let pair = LLVMJITCSymbolMapPair {
                    Name: item.Name,
                    Sym: evaluated_symbol.into(),
                };
                let mut pairs = [pair];
                let materialization_unit =
                    unsafe { LLVMOrcAbsoluteSymbols(pairs.as_mut_ptr(), pairs.len()) };
                let error = unsafe { LLVMOrcJITDylibDefine(dylib.0, materialization_unit) };
                if !error.is_null() {
                    return error;
                }
            }
            std::ptr::null_mut()
        }
    }

    pub trait CustomDefinitionGenerator<Context> {
        fn define_absolute_symbol(
            context: &mut Context,
            lookup_kind: &LookupKind,
            dylib: Dylib,
            dylib_lookup_flags: &DylibLookupFlags,
            symbol_name: &CStr,
            symbol_lookup_flags: &SymbolLookupFlags,
        ) -> Result<EvaluatedSymbol, String>;
    }

    pub struct ThreadSafeModule(LLVMOrcThreadSafeModuleRef);

    impl ThreadSafeModule {
        pub fn new(module: Module, thread_safe_context: ThreadSafeContext) -> Self {
            #[allow(dead_code)]
            struct InkwellModule<'ctx> {
                data_layout: RefCell<Option<DataLayout>>,
                module: Cell<LLVMModuleRef>,
                owned_by_ee: RefCell<Option<ExecutionEngine<'ctx>>>,
                marker: PhantomData<&'ctx Context>,
            }

            let mut module = unsafe { std::mem::transmute::<_, InkwellModule>(module) };
            let context = thread_safe_context.0;
            std::mem::forget(thread_safe_context);

            Self(unsafe { LLVMOrcCreateNewThreadSafeModule(*module.module.get_mut(), context) })
        }
    }

    impl Drop for ThreadSafeModule {
        fn drop(&mut self) {
            unsafe { LLVMOrcDisposeThreadSafeModule(self.0) }
        }
    }
}
