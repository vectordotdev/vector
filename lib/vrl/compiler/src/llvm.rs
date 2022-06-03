use crate::{
    state::{ExternalEnv, LocalEnv},
    Function as StdlibFunction, Program, Resolved,
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
    values::{BasicMetadataValueEnum, CallSiteValue, FunctionValue, GlobalValue, PointerValue},
    OptimizationLevel,
};
use libc::{dlerror, dlsym};
use llvm_sys::orc2::LLVMJITSymbolGenericFlags;
use lookup::LookupBuf;
use parser::ast::Ident;
use std::{collections::HashMap, ffi::CStr, ops::Deref};

static VRL_EXECUTE_SYMBOL: &str = "vrl_execute";

pub struct Compiler(ThreadSafeContext);

impl Compiler {
    pub fn new() -> Result<Self, String> {
        Target::initialize_native(&InitializationConfig::default())?;
        Ok(Self(ThreadSafeContext::create()))
    }

    pub fn compile<'a>(
        self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        program: &Program,
        stdlib: Vec<Box<dyn StdlibFunction>>,
        mut symbols: HashMap<&'static str, usize>,
    ) -> Result<Library<'a>, String> {
        symbols.extend(precompiled::symbols().iter());

        for function in &stdlib {
            if let Some((symbol, address)) = function.symbol() {
                symbols.insert(symbol, address);
            }
        }

        let context = self.0.context();
        let buffer =
            MemoryBuffer::create_from_memory_range(precompiled::LLVM_BITCODE, "precompiled");
        let module = context.create_module_from_ir(buffer).unwrap();
        module.set_name("vrl");

        let builder = context.create_builder();
        let function = module.get_function(VRL_EXECUTE_SYMBOL).ok_or(format!(
            r#"failed getting function "{}" from module"#,
            VRL_EXECUTE_SYMBOL
        ))?;
        let precompiled_functions = PrecompiledFunctions::new(&module);
        let context_ref = function.get_nth_param(0).unwrap().into_pointer_value();
        let result_ref = function.get_nth_param(1).unwrap().into_pointer_value();

        module.strip_debug_info();

        for block in function.get_basic_blocks() {
            block.remove_from_function().unwrap();
        }

        let start = context.append_basic_block(function, "start");
        builder.position_at_end(start);

        let mut context = Context {
            stdlib,
            context,
            module,
            builder,
            function,
            precompiled_functions,
            context_ref,
            result_ref,
            variable_map: Default::default(),
            variables: Default::default(),
            resolved_map: Default::default(),
            resolveds: Default::default(),
            lookup_buf_map: Default::default(),
            lookup_bufs: Default::default(),
        };

        program.emit_llvm(state, &mut context)?;

        for variable_ref in &context.variables {
            context
                .vrl_resolved_drop()
                .build_call(&context.builder, *variable_ref);
        }

        context.builder().build_return(None);

        #[allow(clippy::print_stdout)]
        {
            let dir = std::env::temp_dir();
            let file = [dir, "vrl.ll".into()]
                .iter()
                .collect::<std::path::PathBuf>();

            println!("LLVM IR -> {}", file.display());
            context.module.print_to_file(&file).unwrap();
        }

        if !context.function.verify(true) {
            return Err(format!(
                "Generated code for VRL function failed verification:\n{:?}",
                context.function
            ));
        }

        context.module.verify().unwrap();

        context.optimize().unwrap();

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
                let symbol_map_pairs =
                    SymbolMapPairs::from_iter(symbols.into_iter().map(|(symbol, address)| {
                        let symbol_string_pool_entry = lljit.mangle_and_intern(symbol);
                        let flags = SymbolFlags::new(
                            LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsExported as u8
                                | LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsCallable
                                    as u8,
                            0,
                        );
                        let evaluated_symbol = EvaluatedSymbol::new(address as _, flags);
                        (symbol_string_pool_entry, evaluated_symbol)
                    }));

                MaterializationUnit::from_absolute_symbols(symbol_map_pairs)
            })
            .map_err(|error| error.get_message().to_string())?;

        let definition_generator = Box::new(Wrapper::new(CustomDefinitionGenerator));

        Ok(Library::new(self.0, lljit, definition_generator))
    }
}

pub struct Context<'ctx> {
    stdlib: Vec<Box<dyn StdlibFunction>>,
    context: &'ctx inkwell::context::Context,
    module: Module<'ctx>,
    builder: inkwell::builder::Builder<'ctx>,
    function: FunctionValue<'ctx>,
    precompiled_functions: PrecompiledFunctions<'ctx>,
    context_ref: PointerValue<'ctx>,
    result_ref: PointerValue<'ctx>,
    variable_map: HashMap<Ident, usize>,
    variables: Vec<PointerValue<'ctx>>,
    resolved_map: HashMap<Resolved, usize>,
    resolveds: Vec<GlobalValue<'ctx>>,
    lookup_buf_map: HashMap<LookupBuf, usize>,
    lookup_bufs: Vec<GlobalValue<'ctx>>,
}

impl<'ctx> Context<'ctx> {
    pub fn stdlib(&self, function_id: usize) -> &dyn StdlibFunction {
        self.stdlib[function_id].deref()
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

    pub fn function(&self) -> inkwell::values::FunctionValue<'ctx> {
        self.function
    }

    pub fn context_ref(&self) -> inkwell::values::PointerValue<'ctx> {
        self.context_ref
    }

    pub fn result_ref(&self) -> inkwell::values::PointerValue<'ctx> {
        self.result_ref
    }

    pub fn set_result_ref(&mut self, result_ref: inkwell::values::PointerValue<'ctx>) {
        self.result_ref = result_ref
    }

    pub fn get_or_insert_variable_ref(
        &mut self,
        ident: &Ident,
    ) -> inkwell::values::PointerValue<'ctx> {
        let index = self.variable_map.get(ident).cloned().unwrap_or_else(|| {
            let position = self
                .builder
                .get_insert_block()
                .expect("builder must be positioned at block");
            if let Some(instruction) = self
                .function
                .get_first_basic_block()
                .and_then(|block| block.get_first_instruction())
            {
                self.builder.position_before(&instruction);
            }
            let variable = self.build_alloca_resolved(ident);
            self.vrl_resolved_initialize()
                .build_call(&self.builder, variable);
            self.builder.position_at_end(position);
            let index = self.variables.len();
            self.variables.push(variable);
            self.variable_map.insert(ident.clone(), index);
            index
        });

        self.variables[index]
    }

    pub fn get_variable_ref(&mut self, ident: &Ident) -> inkwell::values::PointerValue<'ctx> {
        let index = self
            .variable_map
            .get(ident)
            .unwrap_or_else(|| panic!(r#"unknown variable "{}""#, ident));
        self.variables[*index]
    }

    pub fn into_const<T: Sized>(&self, value: T, name: &str) -> inkwell::values::GlobalValue<'ctx> {
        let size = std::mem::size_of::<T>();
        let global_type = self.context().i8_type().array_type(size as _);
        let global = self.module.add_global(global_type, None, name);
        global.set_linkage(inkwell::module::Linkage::Private);
        global.set_alignment(std::mem::align_of::<T>() as _);
        let value = self.into_i8_array_value(value);
        global.set_initializer(&value);
        global
    }

    pub fn into_i8_array_value<T: Sized>(&self, value: T) -> inkwell::values::ArrayValue<'ctx> {
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
                    &value as *const _ as *const i8,
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

    pub fn into_resolved_const_ref(
        &mut self,
        resolved: Resolved,
    ) -> inkwell::values::PointerValue<'ctx> {
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

    pub fn into_lookup_buf_const_ref(
        &mut self,
        lookup_buf: LookupBuf,
    ) -> inkwell::values::PointerValue<'ctx> {
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

    pub fn build_alloca_resolved(&self, name: &str) -> inkwell::values::PointerValue<'ctx> {
        let resolved_type = self
            .function
            .get_nth_param(1)
            .unwrap()
            .get_type()
            .into_pointer_type()
            .get_element_type()
            .into_struct_type();
        self.builder.build_alloca(resolved_type, name)
    }

    pub fn resolved_ref_type(&self) -> PointerType<'ctx> {
        self.vrl_resolved_initialize()
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn value_ref_type(&self) -> PointerType<'ctx> {
        self.vrl_value_initialize()
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn optional_value_ref_type(&self) -> PointerType<'ctx> {
        self.vrl_optional_value_initialize()
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn static_ref_type(&self) -> PointerType<'ctx> {
        self.vrl_static_initialize()
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
    }

    pub fn vec_type(&self) -> StructType<'ctx> {
        self.vrl_vec_initialize()
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
            .get_element_type()
            .into_struct_type()
    }

    pub fn btree_map_type(&self) -> StructType<'ctx> {
        self.vrl_btree_map_initialize()
            .function
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value()
            .get_type()
            .get_element_type()
            .into_struct_type()
    }

    pub fn usize_type(&self) -> IntType<'ctx> {
        self.context
            .custom_width_int_type((std::mem::size_of::<usize>() * 8) as _)
    }

    pub fn vrl_resolved_initialize(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_initialize
    }

    pub fn vrl_value_initialize(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_value_initialize
    }

    pub fn vrl_optional_value_initialize(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_optional_value_initialize
    }

    pub fn vrl_static_initialize(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_static_initialize
    }

    pub fn vrl_vec_initialize(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_vec_initialize
    }

    pub fn vrl_btree_map_initialize(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_btree_map_initialize
    }

    pub fn vrl_resolved_swap(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_resolved_swap
    }

    pub fn vrl_resolved_drop(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_drop
    }

    pub fn vrl_optional_value_drop(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_optional_value_drop
    }

    pub fn vrl_resolved_as_value(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_as_value
    }

    pub fn vrl_resolved_as_value_to_optional_value(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions
            .vrl_resolved_as_value_to_optional_value
    }

    pub fn vrl_resolved_err_into_ok(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_err_into_ok
    }

    pub fn vrl_resolved_is_ok(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_is_ok
    }

    pub fn vrl_resolved_is_err(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_is_err
    }

    pub fn vrl_value_boolean_is_true(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_value_boolean_is_true
    }

    pub fn vrl_value_is_falsy(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_value_is_falsy
    }

    pub fn vrl_target_assign(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_target_assign
    }

    pub fn vrl_vec_insert(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_vec_insert
    }

    pub fn vrl_btree_map_insert(&self) -> PrecompiledFunction<'ctx, 4> {
        self.precompiled_functions.vrl_btree_map_insert
    }

    pub fn vrl_resolved_set_null(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_set_null
    }

    pub fn vrl_resolved_set_false(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_resolved_set_false
    }

    pub fn vrl_expression_abort(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_abort
    }

    pub fn vrl_expression_assignment_target_insert_internal_path(
        &self,
    ) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions
            .vrl_expression_assignment_target_insert_internal_path
    }

    pub fn vrl_expression_assignment_target_insert_external(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions
            .vrl_expression_assignment_target_insert_external
    }

    pub fn vrl_expression_literal(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_expression_literal
    }

    pub fn vrl_expression_not(&self) -> PrecompiledFunction<'ctx, 1> {
        self.precompiled_functions.vrl_expression_not
    }

    pub fn vrl_expression_array_set_result(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_expression_array_set_result
    }

    pub fn vrl_expression_object_set_result(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_expression_object_set_result
    }

    pub fn vrl_expression_op_mul_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_mul_integer
    }

    pub fn vrl_expression_op_mul_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_mul_float
    }

    pub fn vrl_expression_op_mul(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_mul
    }

    pub fn vrl_expression_op_div_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_div_integer
    }

    pub fn vrl_expression_op_div_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_div_float
    }

    pub fn vrl_expression_op_div(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_div
    }

    pub fn vrl_expression_op_add_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_add_integer
    }

    pub fn vrl_expression_op_add_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_add_float
    }

    pub fn vrl_expression_op_add_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_add_bytes
    }

    pub fn vrl_expression_op_add(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_add
    }

    pub fn vrl_expression_op_sub_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_sub_integer
    }

    pub fn vrl_expression_op_sub_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_sub_float
    }

    pub fn vrl_expression_op_sub(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_sub
    }

    pub fn vrl_expression_op_rem_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_rem_integer
    }

    pub fn vrl_expression_op_rem_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_rem_float
    }

    pub fn vrl_expression_op_rem(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_rem
    }

    pub fn vrl_expression_op_ne_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ne_integer
    }

    pub fn vrl_expression_op_ne_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ne_float
    }

    pub fn vrl_expression_op_ne_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ne_bytes
    }

    pub fn vrl_expression_op_ne(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ne
    }

    pub fn vrl_expression_op_eq_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_eq_integer
    }

    pub fn vrl_expression_op_eq_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_eq_float
    }

    pub fn vrl_expression_op_eq_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_eq_bytes
    }

    pub fn vrl_expression_op_eq(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_eq
    }

    pub fn vrl_expression_op_ge_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ge_integer
    }

    pub fn vrl_expression_op_ge_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ge_float
    }

    pub fn vrl_expression_op_ge_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ge_bytes
    }

    pub fn vrl_expression_op_ge(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_ge
    }

    pub fn vrl_expression_op_gt_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_gt_integer
    }

    pub fn vrl_expression_op_gt_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_gt_float
    }

    pub fn vrl_expression_op_gt_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_gt_bytes
    }

    pub fn vrl_expression_op_gt(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_gt
    }

    pub fn vrl_expression_op_le_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_le_integer
    }

    pub fn vrl_expression_op_le_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_le_float
    }

    pub fn vrl_expression_op_le_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_le_bytes
    }

    pub fn vrl_expression_op_le(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_le
    }

    pub fn vrl_expression_op_lt_integer(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_lt_integer
    }

    pub fn vrl_expression_op_lt_float(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_lt_float
    }

    pub fn vrl_expression_op_lt_bytes(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_lt_bytes
    }

    pub fn vrl_expression_op_lt(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_lt
    }

    pub fn vrl_expression_op_merge_object(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_merge_object
    }

    pub fn vrl_expression_op_and_truthy(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_expression_op_and_truthy
    }

    pub fn vrl_expression_op_and_falsy(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_expression_op_and_falsy
    }

    pub fn vrl_expression_query_target_external(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions
            .vrl_expression_query_target_external
    }

    pub fn vrl_expression_query_target(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_expression_query_target
    }

    pub fn vrl_expression_function_call(&self) -> PrecompiledFunction<'ctx, 2> {
        self.precompiled_functions.vrl_expression_function_call
    }

    pub fn vrl_del_external(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_del_external
    }

    pub fn vrl_del_internal(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_del_internal
    }

    pub fn vrl_del_expression(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_del_expression
    }

    pub fn vrl_exists_external(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_exists_external
    }

    pub fn vrl_exists_internal(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_exists_internal
    }

    pub fn vrl_exists_expression(&self) -> PrecompiledFunction<'ctx, 3> {
        self.precompiled_functions.vrl_exists_expression
    }

    pub fn optimize(&self) -> Result<(), String> {
        let function_pass_manager = PassManager::create(());
        function_pass_manager.add_function_inlining_pass();
        function_pass_manager.run_on(&self.module);

        let pass_manager = PassManager::create(());
        let pass_manager_builder = PassManagerBuilder::create();
        pass_manager_builder.set_optimization_level(OptimizationLevel::Aggressive);
        pass_manager_builder.populate_module_pass_manager(&pass_manager);

        pass_manager.run_on(&self.module);

        #[allow(clippy::print_stdout)]
        {
            let dir = std::env::temp_dir();
            let file = [dir, "vrl.opt.ll".into()]
                .iter()
                .collect::<std::path::PathBuf>();

            println!("LLVM IR -> {}", file.display());
            self.module.print_to_file(&file).unwrap();
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
            println!("resolving symbol: {:?}", symbol_name);
            #[cfg(target_os = "macos")]
            let symbol_name = if symbol_name.to_string_lossy().starts_with('_') {
                &symbol_name[1..]
            } else {
                symbol_name
            };

            let address = unsafe { dlsym(libc::RTLD_DEFAULT, symbol_name.as_ptr()) };
            if address.is_null() {
                let error = unsafe { dlerror() };
                if !error.is_null() {
                    let error = unsafe { CStr::from_ptr(error) };
                    println!("resolving symbol {:?}, error: {:?}", symbol_name, error);
                    return Err(LLVMError::new_string_error(&error.to_string_lossy()));
                }
            }

            let flags = SymbolFlags::new(
                LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsExported as u8
                    | LLVMJITSymbolGenericFlags::LLVMJITSymbolGenericFlagsCallable as u8,
                0,
            );

            std::mem::forget(symbol_name.clone());

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

pub struct PrecompiledFunctions<'ctx> {
    vrl_resolved_initialize: PrecompiledFunction<'ctx, 1>,
    vrl_value_initialize: PrecompiledFunction<'ctx, 1>,
    vrl_optional_value_initialize: PrecompiledFunction<'ctx, 1>,
    vrl_static_initialize: PrecompiledFunction<'ctx, 1>,
    vrl_vec_initialize: PrecompiledFunction<'ctx, 2>,
    vrl_btree_map_initialize: PrecompiledFunction<'ctx, 2>,
    vrl_resolved_swap: PrecompiledFunction<'ctx, 2>,
    vrl_resolved_drop: PrecompiledFunction<'ctx, 1>,
    vrl_optional_value_drop: PrecompiledFunction<'ctx, 1>,
    vrl_resolved_as_value: PrecompiledFunction<'ctx, 1>,
    vrl_resolved_as_value_to_optional_value: PrecompiledFunction<'ctx, 2>,
    vrl_resolved_err_into_ok: PrecompiledFunction<'ctx, 1>,
    vrl_resolved_is_ok: PrecompiledFunction<'ctx, 1>,
    vrl_resolved_is_err: PrecompiledFunction<'ctx, 1>,
    vrl_value_boolean_is_true: PrecompiledFunction<'ctx, 1>,
    vrl_value_is_falsy: PrecompiledFunction<'ctx, 1>,
    vrl_target_assign: PrecompiledFunction<'ctx, 2>,
    vrl_vec_insert: PrecompiledFunction<'ctx, 3>,
    vrl_btree_map_insert: PrecompiledFunction<'ctx, 4>,
    vrl_resolved_set_null: PrecompiledFunction<'ctx, 1>,
    vrl_resolved_set_false: PrecompiledFunction<'ctx, 1>,
    vrl_expression_abort: PrecompiledFunction<'ctx, 3>,
    vrl_expression_assignment_target_insert_internal_path: PrecompiledFunction<'ctx, 3>,
    vrl_expression_assignment_target_insert_external: PrecompiledFunction<'ctx, 3>,
    vrl_expression_literal: PrecompiledFunction<'ctx, 2>,
    vrl_expression_not: PrecompiledFunction<'ctx, 1>,
    vrl_expression_array_set_result: PrecompiledFunction<'ctx, 2>,
    vrl_expression_object_set_result: PrecompiledFunction<'ctx, 2>,
    vrl_expression_op_mul_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_mul_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_mul: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_div_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_div_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_div: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_add_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_add_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_add_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_add: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_sub_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_sub_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_sub: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_rem_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_rem_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_rem: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ne_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ne_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ne_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ne: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_eq_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_eq_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_eq_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_eq: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ge_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ge_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ge_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_ge: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_gt_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_gt_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_gt_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_gt: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_le_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_le_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_le_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_le: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_lt_integer: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_lt_float: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_lt_bytes: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_lt: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_merge_object: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_and_truthy: PrecompiledFunction<'ctx, 3>,
    vrl_expression_op_and_falsy: PrecompiledFunction<'ctx, 2>,
    vrl_expression_query_target_external: PrecompiledFunction<'ctx, 3>,
    vrl_expression_query_target: PrecompiledFunction<'ctx, 2>,
    vrl_expression_function_call: PrecompiledFunction<'ctx, 2>,
    vrl_del_external: PrecompiledFunction<'ctx, 3>,
    vrl_del_internal: PrecompiledFunction<'ctx, 3>,
    vrl_del_expression: PrecompiledFunction<'ctx, 3>,
    vrl_exists_external: PrecompiledFunction<'ctx, 3>,
    vrl_exists_internal: PrecompiledFunction<'ctx, 3>,
    vrl_exists_expression: PrecompiledFunction<'ctx, 3>,
}

impl<'ctx> PrecompiledFunctions<'ctx> {
    pub fn new(module: &Module<'ctx>) -> Self {
        Self {
            vrl_resolved_initialize: PrecompiledFunction {
                function: module.get_function("vrl_resolved_initialize").unwrap(),
            },
            vrl_value_initialize: PrecompiledFunction {
                function: module.get_function("vrl_value_initialize").unwrap(),
            },
            vrl_optional_value_initialize: PrecompiledFunction {
                function: module
                    .get_function("vrl_optional_value_initialize")
                    .unwrap(),
            },
            vrl_static_initialize: PrecompiledFunction {
                function: module.get_function("vrl_static_initialize").unwrap(),
            },
            vrl_vec_initialize: PrecompiledFunction {
                function: module.get_function("vrl_vec_initialize").unwrap(),
            },
            vrl_btree_map_initialize: PrecompiledFunction {
                function: module.get_function("vrl_btree_map_initialize").unwrap(),
            },
            vrl_resolved_swap: PrecompiledFunction {
                function: module.get_function("vrl_resolved_swap").unwrap(),
            },
            vrl_resolved_drop: PrecompiledFunction {
                function: module.get_function("vrl_resolved_drop").unwrap(),
            },
            vrl_optional_value_drop: PrecompiledFunction {
                function: module.get_function("vrl_optional_value_drop").unwrap(),
            },
            vrl_resolved_as_value: PrecompiledFunction {
                function: module.get_function("vrl_resolved_as_value").unwrap(),
            },
            vrl_resolved_as_value_to_optional_value: PrecompiledFunction {
                function: module
                    .get_function("vrl_resolved_as_value_to_optional_value")
                    .unwrap(),
            },
            vrl_resolved_err_into_ok: PrecompiledFunction {
                function: module.get_function("vrl_resolved_err_into_ok").unwrap(),
            },
            vrl_resolved_is_ok: PrecompiledFunction {
                function: module.get_function("vrl_resolved_is_ok").unwrap(),
            },
            vrl_resolved_is_err: PrecompiledFunction {
                function: module.get_function("vrl_resolved_is_err").unwrap(),
            },
            vrl_value_boolean_is_true: PrecompiledFunction {
                function: module.get_function("vrl_value_boolean_is_true").unwrap(),
            },
            vrl_value_is_falsy: PrecompiledFunction {
                function: module.get_function("vrl_value_is_falsy").unwrap(),
            },
            vrl_target_assign: PrecompiledFunction {
                function: module.get_function("vrl_target_assign").unwrap(),
            },
            vrl_vec_insert: PrecompiledFunction {
                function: module.get_function("vrl_vec_insert").unwrap(),
            },
            vrl_btree_map_insert: PrecompiledFunction {
                function: module.get_function("vrl_btree_map_insert").unwrap(),
            },
            vrl_resolved_set_null: PrecompiledFunction {
                function: module.get_function("vrl_resolved_set_null").unwrap(),
            },
            vrl_resolved_set_false: PrecompiledFunction {
                function: module.get_function("vrl_resolved_set_false").unwrap(),
            },
            vrl_expression_abort: PrecompiledFunction {
                function: module.get_function("vrl_expression_abort").unwrap(),
            },
            vrl_expression_assignment_target_insert_internal_path: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_assignment_target_insert_internal_path")
                    .unwrap(),
            },
            vrl_expression_assignment_target_insert_external: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_assignment_target_insert_external")
                    .unwrap(),
            },
            vrl_expression_literal: PrecompiledFunction {
                function: module.get_function("vrl_expression_literal").unwrap(),
            },
            vrl_expression_not: PrecompiledFunction {
                function: module.get_function("vrl_expression_not").unwrap(),
            },
            vrl_expression_array_set_result: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_array_set_result")
                    .unwrap(),
            },
            vrl_expression_object_set_result: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_object_set_result")
                    .unwrap(),
            },
            vrl_expression_op_mul_integer: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_op_mul_integer")
                    .unwrap(),
            },
            vrl_expression_op_mul_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_mul_float").unwrap(),
            },
            vrl_expression_op_mul: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_mul").unwrap(),
            },
            vrl_expression_op_div_integer: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_op_div_integer")
                    .unwrap(),
            },
            vrl_expression_op_div_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_div_float").unwrap(),
            },
            vrl_expression_op_div: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_div").unwrap(),
            },
            vrl_expression_op_add_integer: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_op_add_integer")
                    .unwrap(),
            },
            vrl_expression_op_add_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_add_float").unwrap(),
            },
            vrl_expression_op_add_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_add_bytes").unwrap(),
            },
            vrl_expression_op_add: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_add").unwrap(),
            },
            vrl_expression_op_sub_integer: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_op_sub_integer")
                    .unwrap(),
            },
            vrl_expression_op_sub_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_sub_float").unwrap(),
            },
            vrl_expression_op_sub: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_sub").unwrap(),
            },
            vrl_expression_op_rem_integer: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_op_rem_integer")
                    .unwrap(),
            },
            vrl_expression_op_rem_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_rem_float").unwrap(),
            },
            vrl_expression_op_rem: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_rem").unwrap(),
            },
            vrl_expression_op_ne_integer: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ne_integer").unwrap(),
            },
            vrl_expression_op_ne_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ne_float").unwrap(),
            },
            vrl_expression_op_ne_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ne_bytes").unwrap(),
            },
            vrl_expression_op_ne: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ne").unwrap(),
            },
            vrl_expression_op_eq_integer: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_eq_integer").unwrap(),
            },
            vrl_expression_op_eq_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_eq_float").unwrap(),
            },
            vrl_expression_op_eq_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_eq_bytes").unwrap(),
            },
            vrl_expression_op_eq: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_eq").unwrap(),
            },
            vrl_expression_op_ge_integer: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ge_integer").unwrap(),
            },
            vrl_expression_op_ge_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ge_float").unwrap(),
            },
            vrl_expression_op_ge_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ge_bytes").unwrap(),
            },
            vrl_expression_op_ge: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_ge").unwrap(),
            },
            vrl_expression_op_gt_integer: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_gt_integer").unwrap(),
            },
            vrl_expression_op_gt_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_gt_float").unwrap(),
            },
            vrl_expression_op_gt_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_gt_bytes").unwrap(),
            },
            vrl_expression_op_gt: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_gt").unwrap(),
            },
            vrl_expression_op_le_integer: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_le_integer").unwrap(),
            },
            vrl_expression_op_le_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_le_float").unwrap(),
            },
            vrl_expression_op_le_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_le_bytes").unwrap(),
            },
            vrl_expression_op_le: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_le").unwrap(),
            },
            vrl_expression_op_lt_integer: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_lt_integer").unwrap(),
            },
            vrl_expression_op_lt_float: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_lt_float").unwrap(),
            },
            vrl_expression_op_lt_bytes: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_lt_bytes").unwrap(),
            },
            vrl_expression_op_lt: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_lt").unwrap(),
            },
            vrl_expression_op_merge_object: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_op_merge_object")
                    .unwrap(),
            },
            vrl_expression_op_and_truthy: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_and_truthy").unwrap(),
            },
            vrl_expression_op_and_falsy: PrecompiledFunction {
                function: module.get_function("vrl_expression_op_and_falsy").unwrap(),
            },
            vrl_expression_query_target_external: PrecompiledFunction {
                function: module
                    .get_function("vrl_expression_query_target_external")
                    .unwrap(),
            },
            vrl_expression_query_target: PrecompiledFunction {
                function: module.get_function("vrl_expression_query_target").unwrap(),
            },
            vrl_expression_function_call: PrecompiledFunction {
                function: module.get_function("vrl_expression_function_call").unwrap(),
            },
            vrl_del_internal: PrecompiledFunction {
                function: module.get_function("vrl_del_internal").unwrap(),
            },
            vrl_del_external: PrecompiledFunction {
                function: module.get_function("vrl_del_external").unwrap(),
            },
            vrl_del_expression: PrecompiledFunction {
                function: module.get_function("vrl_del_expression").unwrap(),
            },
            vrl_exists_internal: PrecompiledFunction {
                function: module.get_function("vrl_exists_internal").unwrap(),
            },
            vrl_exists_external: PrecompiledFunction {
                function: module.get_function("vrl_exists_external").unwrap(),
            },
            vrl_exists_expression: PrecompiledFunction {
                function: module.get_function("vrl_exists_expression").unwrap(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PrecompiledFunction<'ctx, const N: usize> {
    pub function: FunctionValue<'ctx>,
}

impl<'ctx> PrecompiledFunction<'ctx, 1> {
    pub fn build_call(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        arg1: impl Into<BasicMetadataValueEnum<'ctx>>,
    ) -> CallSiteValue<'ctx> {
        builder.build_call(
            self.function,
            &[arg1.into()],
            &self.function.get_name().to_string_lossy(),
        )
    }
}

impl<'ctx> PrecompiledFunction<'ctx, 2> {
    pub fn build_call(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        arg1: impl Into<BasicMetadataValueEnum<'ctx>>,
        arg2: impl Into<BasicMetadataValueEnum<'ctx>>,
    ) -> CallSiteValue<'ctx> {
        builder.build_call(
            self.function,
            &[arg1.into(), arg2.into()],
            &self.function.get_name().to_string_lossy(),
        )
    }
}

impl<'ctx> PrecompiledFunction<'ctx, 3> {
    pub fn build_call(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        arg1: impl Into<BasicMetadataValueEnum<'ctx>>,
        arg2: impl Into<BasicMetadataValueEnum<'ctx>>,
        arg3: impl Into<BasicMetadataValueEnum<'ctx>>,
    ) -> CallSiteValue<'ctx> {
        builder.build_call(
            self.function,
            &[arg1.into(), arg2.into(), arg3.into()],
            &self.function.get_name().to_string_lossy(),
        )
    }
}

impl<'ctx> PrecompiledFunction<'ctx, 4> {
    pub fn build_call(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        arg1: impl Into<BasicMetadataValueEnum<'ctx>>,
        arg2: impl Into<BasicMetadataValueEnum<'ctx>>,
        arg3: impl Into<BasicMetadataValueEnum<'ctx>>,
        arg4: impl Into<BasicMetadataValueEnum<'ctx>>,
    ) -> CallSiteValue<'ctx> {
        builder.build_call(
            self.function,
            &[arg1.into(), arg2.into(), arg3.into(), arg4.into()],
            &self.function.get_name().to_string_lossy(),
        )
    }
}
