use crate::{
    function::Symbol,
    state::{ExternalEnv, LocalEnv},
    Expression, Function as StdlibFunction, Program, Resolved,
};
use inkwell::{
    error::LLVMError,
    memory_buffer::MemoryBuffer,
    module::Module,
    orc2::{
        lljit::{Function, LLJITBuilder, LLJIT},
        CAPIDefinitionGenerator, DefinitionGenerator, EvaluatedSymbol, JITDylib,
        JITDylibLookupFlags, JITTargetMachineBuilder, LookupKind, MaterializationUnit, SymbolFlags,
        SymbolMapPairs, ThreadSafeContext, Wrapper,
    },
    passes::{PassManager, PassManagerBuilder},
    targets::{InitializationConfig, Target},
    types::{IntType, PointerType, StructType},
    values::{
        ArrayValue, BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, GlobalValue,
        InstructionValue, IntValue, PointerValue,
    },
};
use libc::{dlerror, dlsym};
use llvm_sys::orc2::LLVMJITSymbolGenericFlags;
use lookup::LookupBuf;
use parser::ast::Ident;
use precompiled::PrecompiledFunctions;
use std::{collections::HashMap, ffi::CStr};

pub use inkwell::{basic_block::BasicBlock, OptimizationLevel};

static VRL_EXECUTE_SYMBOL: &str = "vrl_execute";

pub struct Compiler(ThreadSafeContext);

impl Compiler {
    pub fn new() -> Result<Self, String> {
        Target::initialize_native(&InitializationConfig::default())?;
        Ok(Self(ThreadSafeContext::create()))
    }

    pub fn compile<'a>(
        self,
        optimization_level: OptimizationLevel,
        state: (&LocalEnv, &ExternalEnv),
        program: &Program,
        stdlib: &[Box<dyn StdlibFunction>],
        mut symbols: HashMap<&'static str, usize>,
    ) -> Result<Library<'a>, String> {
        symbols.extend(precompiled::symbols().iter());

        for function in stdlib {
            if let Some(Symbol { name, address, .. }) = function.symbol() {
                symbols.insert(name, address);
            }
        }

        let context = self.0.context();
        let buffer =
            MemoryBuffer::create_from_memory_range(precompiled::LLVM_BITCODE, "precompiled");
        let module = context.create_module_from_ir(buffer).unwrap();
        module.set_name("vrl");

        let builder = context.create_builder();
        let fns = PrecompiledFunctions::new(&module)?;
        let function = fns.vrl_execute.function;
        let context_ref = function.get_nth_param(0).unwrap().into_pointer_value();
        let result_ref = function.get_nth_param(1).unwrap().into_pointer_value();

        module.strip_debug_info();

        for block in function.get_basic_blocks() {
            block.remove_from_function().unwrap();
        }

        let start_block = context.append_basic_block(function, "start");
        let end_block = context.append_basic_block(function, "end");
        builder.position_at_end(start_block);

        let mut context = Context {
            stdlib,
            context,
            module,
            builder,
            function,
            fns,
            context_ref,
            global_result_ref: result_ref,
            result_ref,
            variable_map: HashMap::default(),
            variables: Vec::default(),
            resolved_map: HashMap::default(),
            resolveds: Vec::default(),
            lookup_buf_map: HashMap::default(),
            lookup_bufs: Vec::default(),
            discard_error: false,
        };

        program.emit_llvm(state, &mut context)?;
        context.build_unconditional_branch(end_block);
        context.position_at_end(end_block);

        for variable_ref in &context.variables {
            context
                .fns
                .vrl_resolved_drop
                .build_call(&context.builder, *variable_ref);
        }

        context.builder().build_return(None);

        #[allow(clippy::print_stdout)]
        {
            let file = tempfile::Builder::new()
                .suffix(".ll")
                .tempfile()
                .map_err(|error| error.to_string())?;
            let path = file.path();
            println!("LLVM IR -> {}", path.display());
            context.module.print_to_file(path).unwrap();
            file.keep().unwrap();
        }

        if !context.function.verify(true) {
            return Err(format!(
                "Generated code for VRL function failed verification:\n{:?}",
                context.function
            ));
        }

        context.module.verify().unwrap();

        if optimization_level != OptimizationLevel::None {
            context.optimize(optimization_level).unwrap();
        }

        let jit_target_machine_builder = JITTargetMachineBuilder::detect_host().unwrap();
        let lljit = LLJITBuilder::create()
            .set_jit_target_machine_builder(jit_target_machine_builder)
            .build()
            .unwrap();
        let dylib = lljit.get_main_jit_dylib();

        lljit
            .add_module(&dylib, self.0.create_module(context.consume()))
            .unwrap();

        lljit
            .get_main_jit_dylib()
            .define({
                let symbol_map_pairs = symbols
                    .into_iter()
                    .map(|(symbol, address)| {
                        let symbol_string_pool_entry = lljit.mangle_and_intern(symbol);
                        let flags = SymbolFlags::new(
                            LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsExported as u8
                                | LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsCallable
                                    as u8,
                            0,
                        );
                        let evaluated_symbol = EvaluatedSymbol::new(address as _, flags);
                        (symbol_string_pool_entry, evaluated_symbol)
                    })
                    .collect::<SymbolMapPairs>();

                MaterializationUnit::from_absolute_symbols(symbol_map_pairs)
            })
            .map_err(|error| error.get_message().to_string())?;

        let definition_generator = Box::new(Wrapper::new(CustomDefinitionGenerator));

        Ok(Library::new(self.0, lljit, definition_generator))
    }
}

pub struct Context<'ctx> {
    stdlib: &'ctx [Box<dyn StdlibFunction>],
    context: &'ctx inkwell::context::Context,
    module: Module<'ctx>,
    builder: inkwell::builder::Builder<'ctx>,
    function: FunctionValue<'ctx>,
    fns: PrecompiledFunctions<'ctx>,
    context_ref: PointerValue<'ctx>,
    global_result_ref: PointerValue<'ctx>,
    result_ref: PointerValue<'ctx>,
    variable_map: HashMap<Ident, usize>,
    variables: Vec<PointerValue<'ctx>>,
    resolved_map: HashMap<Resolved, usize>,
    resolveds: Vec<GlobalValue<'ctx>>,
    lookup_buf_map: HashMap<LookupBuf, usize>,
    lookup_bufs: Vec<GlobalValue<'ctx>>,
    discard_error: bool,
}

impl<'ctx> Context<'ctx> {
    pub fn stdlib(&self, function_id: usize) -> &dyn StdlibFunction {
        &*self.stdlib[function_id]
    }

    pub fn context(&self) -> &'ctx inkwell::context::Context {
        self.context
    }

    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    pub fn builder(&self) -> &inkwell::builder::Builder<'ctx> {
        &self.builder
    }

    pub fn function(&self) -> FunctionValue<'ctx> {
        self.function
    }

    pub fn fns(&self) -> &PrecompiledFunctions<'ctx> {
        &self.fns
    }

    pub fn context_ref(&self) -> PointerValue<'ctx> {
        self.context_ref
    }

    pub fn global_result_ref(&self) -> PointerValue<'ctx> {
        self.global_result_ref
    }

    pub fn result_ref(&self) -> PointerValue<'ctx> {
        self.result_ref
    }

    pub fn set_result_ref(&mut self, result_ref: PointerValue<'ctx>) {
        self.result_ref = result_ref
    }

    pub fn discard_error(&self) -> bool {
        self.discard_error
    }

    pub fn set_discard_error(&mut self, discard_error: bool) {
        self.discard_error = discard_error;
    }

    pub fn get_or_insert_variable_ref(&mut self, ident: &Ident) -> PointerValue<'ctx> {
        let index = self.variable_map.get(ident).copied().unwrap_or_else(|| {
            let position = self
                .builder
                .get_insert_block()
                .expect("builder must be positioned at block");
            if let Some(instruction) = self
                .function
                .get_first_basic_block()
                .and_then(BasicBlock::get_first_instruction)
            {
                self.builder.position_before(&instruction);
            }
            let variable = self.build_alloca_resolved_initialized(ident);
            self.builder.position_at_end(position);
            let index = self.variables.len();
            self.variables.push(variable);
            self.variable_map.insert(ident.clone(), index);
            index
        });

        self.variables[index]
    }

    pub fn get_variable_ref(&mut self, ident: &Ident) -> PointerValue<'ctx> {
        let index = self
            .variable_map
            .get(ident)
            .unwrap_or_else(|| panic!(r#"unknown variable "{}""#, ident));
        self.variables[*index]
    }

    pub fn into_const<T: Sized>(&self, value: T, name: &str) -> GlobalValue<'ctx> {
        let size = std::mem::size_of::<T>();
        let global_type = self.context().i8_type().array_type(size as _);
        let global = self.module.add_global(global_type, None, name);
        global.set_linkage(inkwell::module::Linkage::Private);
        global.set_alignment(std::mem::align_of::<T>() as _);
        let value = self.into_i8_array_value(value);
        global.set_initializer(&value);
        global
    }

    pub fn into_i8_array_value<T: Sized>(&self, value: T) -> ArrayValue<'ctx> {
        // Rust can't compute the size of generic arguments in a `const` context yet:
        // https://github.com/rust-lang/rust/issues/43408.
        let size = std::mem::size_of::<T>();
        let bytes = {
            // Workaround for not being able to use `std::mem::transmute` here yet:
            // https://github.com/rust-lang/rust/issues/62875
            // https://github.com/rust-lang/rust/issues/61956
            let mut bytes = Vec::<i8>::new();
            bytes.resize(size, 0);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    std::ptr::addr_of!(value).cast::<i8>(),
                    bytes.as_mut_ptr(),
                    size,
                )
            };
            std::mem::forget(value);
            bytes
        };

        let array = bytes
            .into_iter()
            .map(|byte| self.context().i8_type().const_int(byte as _, false))
            .collect::<Vec<_>>();
        self.context().i8_type().const_array(array.as_slice())
    }

    pub fn into_resolved_const_ref(&mut self, resolved: Resolved) -> PointerValue<'ctx> {
        let index = match self.resolved_map.get(&resolved) {
            Some(index) => *index,
            None => {
                let index = self.resolveds.len();
                let name = format!("{:?}", resolved);
                let global = self.into_const(resolved.clone(), &name);
                self.resolved_map.insert(resolved, index);
                self.resolveds.push(global);
                index
            }
        };

        self.resolveds[index].as_pointer_value()
    }

    pub fn into_lookup_buf_const_ref(&mut self, lookup_buf: LookupBuf) -> PointerValue<'ctx> {
        let index = match self.lookup_buf_map.get(&lookup_buf) {
            Some(index) => *index,
            None => {
                let index = self.lookup_bufs.len();
                let name = format!("{}", lookup_buf);
                let global = self.into_const(lookup_buf.clone(), &name);
                self.lookup_buf_map.insert(lookup_buf, index);
                self.lookup_bufs.push(global);
                index
            }
        };

        self.lookup_bufs[index].as_pointer_value()
    }

    pub fn append_basic_block(&self, name: &str) -> BasicBlock<'ctx> {
        self.context.append_basic_block(self.function, name)
    }

    pub fn build_unconditional_branch(
        &self,
        destination_block: BasicBlock<'ctx>,
    ) -> InstructionValue<'ctx> {
        self.builder.build_unconditional_branch(destination_block)
    }

    pub fn build_conditional_branch(
        &self,
        comparison: IntValue<'ctx>,
        then_block: BasicBlock<'ctx>,
        else_block: BasicBlock<'ctx>,
    ) -> InstructionValue<'ctx> {
        self.builder
            .build_conditional_branch(comparison, then_block, else_block)
    }

    pub fn position_at_end(&self, basic_block: BasicBlock<'ctx>) {
        self.builder.position_at_end(basic_block)
    }

    pub fn build_alloca_resolved_initialized(&self, name: &str) -> PointerValue<'ctx> {
        let resolved_type = self
            .resolved_mut_type()
            .get_element_type()
            .into_struct_type();
        let alloca = self.builder.build_alloca(resolved_type, name);
        self.fns.vrl_resolved_initialize.build_call(
            &self.builder,
            self.builder
                .build_bitcast(alloca, self.resolved_maybe_uninit_mut_type(), ""),
        );
        alloca
    }

    pub fn build_alloca_optional_value_initialized(&self, name: &str) -> PointerValue<'ctx> {
        let optional_value_type = self
            .optional_value_mut_type()
            .get_element_type()
            .into_struct_type();
        let alloca = self.builder.build_alloca(optional_value_type, name);
        self.fns.vrl_optional_value_initialize.build_call(
            &self.builder,
            self.builder
                .build_bitcast(alloca, self.optional_value_maybe_uninit_mut_type(), ""),
        );
        alloca
    }

    pub fn build_alloca_vec_initialized(&self, name: &str, capacity: usize) -> PointerValue<'ctx> {
        let vec_type = self.vec_mut_type().get_element_type().into_struct_type();
        let alloca = self.builder.build_alloca(vec_type, name);
        self.fns.vrl_vec_initialize.build_call(
            &self.builder,
            self.cast_vec_maybe_uninit_mut_type(alloca),
            self.usize_type().const_int(capacity as _, false),
        );
        alloca
    }

    pub fn build_alloca_btree_map_initialized(
        &self,
        name: &str,
        capacity: usize,
    ) -> PointerValue<'ctx> {
        let btree_map_type = self
            .btree_map_mut_type()
            .get_element_type()
            .into_struct_type();
        let alloca = self.builder.build_alloca(btree_map_type, name);
        self.fns.vrl_btree_map_initialize.build_call(
            &self.builder,
            self.cast_btree_map_maybe_uninit_mut_type(alloca),
            self.usize_type().const_int(capacity as _, false),
        );
        alloca
    }

    pub fn emit_llvm_abortable(
        &mut self,
        expression: &dyn Expression,
        state: (&LocalEnv, &ExternalEnv),
        resolved_ref: PointerValue<'ctx>,
        abort_end_block: BasicBlock<'ctx>,
        abort_drop_refs: Vec<(
            BasicMetadataValueEnum<'ctx>,
            precompiled::PrecompiledFunction<'ctx, 1>,
        )>,
    ) -> Result<(), String> {
        self.emit_llvm_for_ref(expression, state, resolved_ref)?;

        let type_def = expression.type_def(state);
        if type_def.is_abortable() {
            self.handle_abort(
                resolved_ref.into(),
                self.result_ref.into(),
                abort_end_block,
                abort_drop_refs,
            );
        }

        Ok(())
    }

    pub fn emit_llvm_for_ref(
        &mut self,
        expression: &dyn Expression,
        state: (&LocalEnv, &ExternalEnv),
        resolved_ref: PointerValue<'ctx>,
    ) -> Result<(), String> {
        let result_ref = self.result_ref;
        self.set_result_ref(resolved_ref);
        expression.emit_llvm(state, self)?;
        self.set_result_ref(result_ref);
        Ok(())
    }

    pub fn handle_abort(
        &self,
        resolved_ref: BasicMetadataValueEnum<'ctx>,
        abort_ref: BasicMetadataValueEnum<'ctx>,
        abort_end_block: BasicBlock<'ctx>,
        abort_drop_refs: Vec<(
            BasicMetadataValueEnum<'ctx>,
            precompiled::PrecompiledFunction<'ctx, 1>,
        )>,
    ) {
        let abort_continue_block = self.append_basic_block("abort_continue");
        let abort_abort_block = self.append_basic_block("abort_abort");

        let is_err = self
            .fns()
            .vrl_resolved_is_abort
            .build_call(self.builder(), resolved_ref)
            .try_as_basic_value()
            .left()
            .expect("result is not a basic value")
            .try_into()
            .expect("result is not an int value");

        self.builder()
            .build_conditional_branch(is_err, abort_abort_block, abort_continue_block);

        self.position_at_end(abort_abort_block);
        if resolved_ref != abort_ref {
            self.fns
                .vrl_resolved_swap
                .build_call(&self.builder, resolved_ref, abort_ref);
        }
        for (drop_ref, drop_fn) in abort_drop_refs {
            drop_fn.build_call(self.builder(), drop_ref);
        }
        self.build_unconditional_branch(abort_end_block);

        self.position_at_end(abort_continue_block);
    }

    pub fn context_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_execute
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn resolved_ref_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn resolved_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(1)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn resolved_maybe_uninit_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(2)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn value_ref_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(3)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn value_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(4)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn optional_value_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(5)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn optional_value_maybe_uninit_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(6)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn static_argument_ref_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(7)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn vec_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(8)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn vec_type(&self) -> StructType<'ctx> {
        self.vec_mut_type().get_element_type().into_struct_type()
    }

    pub fn vec_maybe_uninit_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(9)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn btree_map_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(10)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn btree_map_type(&self) -> StructType<'ctx> {
        self.btree_map_mut_type()
            .get_element_type()
            .into_struct_type()
    }

    pub fn btree_map_maybe_uninit_mut_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(11)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn span_ref_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(12)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn lookup_buf_ref_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(13)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn string_ref_type(&self) -> PointerType<'ctx> {
        self.fns
            .vrl_types
            .function
            .get_nth_param(14)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn usize_type(&self) -> IntType<'ctx> {
        self.context
            .custom_width_int_type((std::mem::size_of::<usize>() * 8) as _)
    }

    pub fn cast_resolved_ref_type<V: BasicValue<'ctx>>(&self, value: V) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.resolved_ref_type(), "")
    }

    pub fn cast_value_ref_type<V: BasicValue<'ctx>>(&self, value: V) -> BasicValueEnum<'ctx> {
        self.builder.build_bitcast(value, self.value_ref_type(), "")
    }

    pub fn cast_optional_value_maybe_uninit_mut_type<V: BasicValue<'ctx>>(
        &self,
        value: V,
    ) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.optional_value_maybe_uninit_mut_type(), "")
    }

    pub fn cast_static_argument_ref_type<V: BasicValue<'ctx>>(
        &self,
        value: V,
    ) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.static_argument_ref_type(), "")
    }

    pub fn cast_vec_maybe_uninit_mut_type<V: BasicValue<'ctx>>(
        &self,
        value: V,
    ) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.vec_maybe_uninit_mut_type(), "")
    }

    pub fn cast_btree_map_maybe_uninit_mut_type<V: BasicValue<'ctx>>(
        &self,
        value: V,
    ) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.btree_map_maybe_uninit_mut_type(), "")
    }

    pub fn cast_span_ref_type<V: BasicValue<'ctx>>(&self, value: V) -> BasicValueEnum<'ctx> {
        self.builder.build_bitcast(value, self.span_ref_type(), "")
    }

    pub fn cast_lookup_buf_ref_type<V: BasicValue<'ctx>>(&self, value: V) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.lookup_buf_ref_type(), "")
    }

    pub fn cast_string_ref_type<V: BasicValue<'ctx>>(&self, value: V) -> BasicValueEnum<'ctx> {
        self.builder
            .build_bitcast(value, self.string_ref_type(), "")
    }

    pub fn optimize(&self, level: OptimizationLevel) -> Result<(), String> {
        let function_pass_manager = PassManager::create(());
        function_pass_manager.add_function_inlining_pass();
        function_pass_manager.run_on(&self.module);

        let pass_manager = PassManager::create(());
        let pass_manager_builder = PassManagerBuilder::create();
        pass_manager_builder.set_optimization_level(level);
        pass_manager_builder.populate_module_pass_manager(&pass_manager);

        pass_manager.run_on(&self.module);

        #[allow(clippy::print_stdout)]
        {
            let file = tempfile::Builder::new()
                .suffix(".ll")
                .tempfile()
                .map_err(|error| error.to_string())?;
            let path = file.path();
            println!("LLVM IR -> {}", path.display());
            self.module.print_to_file(path).unwrap();
            file.keep().unwrap();
        }

        if !self.function.verify(false) {
            self.function.print_to_stderr();
            Err("Generated code for VRL function failed verification".into())
        } else {
            Ok(())
        }
    }

    fn consume(self) -> Module<'ctx> {
        self.module
    }
}

#[derive(Debug)]
struct CustomDefinitionGenerator;

impl CAPIDefinitionGenerator for CustomDefinitionGenerator {
    fn try_to_generate(
        &mut self,
        _definition_generator: inkwell::orc2::DefinitionGeneratorRef,
        _lookup_state: &mut inkwell::orc2::LookupState,
        _lookup_kind: LookupKind,
        jit_dylib: JITDylib,
        _jit_dylib_lookup_flags: JITDylibLookupFlags,
        c_lookup_set: inkwell::orc2::CLookupSet,
    ) -> Result<(), LLVMError> {
        for c_lookup in &*c_lookup_set {
            let symbol_string_pool_entry = c_lookup.get_name();
            let _symbol_flags = c_lookup.get_flags();
            let symbol_name = symbol_string_pool_entry.get_string();
            #[cfg(target_os = "macos")]
            let symbol_name = if symbol_name.to_string_lossy().starts_with('_') {
                &symbol_name[1..]
            } else {
                symbol_name
            };

            let address = unsafe { dlsym(libc::RTLD_DEFAULT, symbol_name.as_ptr()) };
            if address.is_null() && !symbol_name.to_string_lossy().contains("proc_macro") {
                let error = unsafe { dlerror() };
                if !error.is_null() {
                    let error = unsafe { CStr::from_ptr(error) };
                    #[allow(clippy::print_stderr)]
                    {
                        eprintln!("resolving symbol {:?}, error: {:?}", symbol_name, error);
                    }
                    return Err(LLVMError::new_string_error(&error.to_string_lossy()));
                }
            }

            let flags = SymbolFlags::new(
                LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsExported as u8
                    | LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsCallable as u8,
                0,
            );

            std::mem::forget(symbol_string_pool_entry.clone());

            let symbol = EvaluatedSymbol::new(address as _, flags);
            let symbols = SymbolMapPairs::from_iter([(symbol_string_pool_entry, symbol)]);

            jit_dylib.define(MaterializationUnit::from_absolute_symbols(symbols))?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Library<'jit>(
    ThreadSafeContext,
    LLJIT<'jit>,
    Box<Wrapper<'jit, CustomDefinitionGenerator>>,
);

impl<'jit> Library<'jit> {
    fn new(
        thread_safe_context: ThreadSafeContext,
        lljit: LLJIT<'jit>,
        mut definition_generator: Box<Wrapper<'jit, CustomDefinitionGenerator>>,
    ) -> Self {
        let dylib = lljit.get_main_jit_dylib();
        let generator = DefinitionGenerator::create_custom_capi_definition_generator(unsafe {
            &mut *(definition_generator.as_mut() as *mut _)
        });
        dylib.add_generator(generator);
        Self(thread_safe_context, lljit, definition_generator)
    }

    pub fn get_function(
        &self,
    ) -> Result<
        Function<for<'a> unsafe extern "C" fn(&'a mut core::Context<'a>, &'a mut crate::Resolved)>,
        LLVMError,
    > {
        unsafe { self.1.get_function(VRL_EXECUTE_SYMBOL) }.map(
            |function: Function<unsafe extern "C" fn()>| unsafe { std::mem::transmute(function) },
        )
    }

    pub fn get_function_address(&self) -> Result<usize, LLVMError> {
        Ok(self.1.get_function_address(VRL_EXECUTE_SYMBOL)? as usize)
    }
}
