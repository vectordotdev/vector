use crate::{Program, Resolved};
use inkwell::{
    execution_engine::{ExecutionEngine, JitFunction},
    memory_buffer::MemoryBuffer,
    module::Module,
    passes::PassManager,
    values::{FunctionValue, GlobalValue, PointerValue},
    OptimizationLevel,
};
use lookup::LookupBuf;
use parser::ast::Ident;
use std::collections::HashMap;

static PRECOMPILED: &'static [u8] = include_bytes!(concat!(env!("OUT_DIR"), "/precompiled.bc"));
static VRL_EXECUTE_SYMBOL: &'static str = "vrl_execute";

pub struct Builder(inkwell::context::Context);

impl Builder {
    pub fn new() -> Result<Self, String> {
        inkwell::targets::Target::initialize_native(
            &inkwell::targets::InitializationConfig::default(),
        )?;

        Ok(Self(inkwell::context::Context::create()))
    }

    pub fn compile(
        &self,
        state: &crate::state::Compiler,
        program: &Program,
    ) -> Result<Context, String> {
        let context = &self.0;
        let buffer = MemoryBuffer::create_from_memory_range(PRECOMPILED, "precompiled");
        let module = Module::parse_bitcode_from_buffer(&buffer, context)
            .map_err(|string| string.to_string())?;
        module.set_name("vrl");
        let builder = context.create_builder();
        let function = module.get_function(VRL_EXECUTE_SYMBOL).ok_or(format!(
            r#"failed getting function "{}" from module"#,
            VRL_EXECUTE_SYMBOL
        ))?;
        let context_ref = function.get_nth_param(0).unwrap().into_pointer_value();
        let result_ref = function.get_nth_param(1).unwrap().into_pointer_value();

        for block in function.get_basic_blocks() {
            block.remove_from_function().unwrap();
        }

        let start = context.append_basic_block(function, "start");
        builder.position_at_end(start);

        let execution_engine = module
            .create_jit_execution_engine(OptimizationLevel::Aggressive)
            .map_err(|string| string.to_string())?;

        let mut context = Context {
            context,
            module,
            builder,
            function,
            execution_engine,
            context_ref,
            result_ref,
            resolved_map: Default::default(),
            resolveds: Default::default(),
            ident_map: Default::default(),
            idents: Default::default(),
            lookup_buf_map: Default::default(),
            lookup_bufs: Default::default(),
        };

        for expression in program.iter() {
            expression.emit_llvm(&state, &mut context)?;
        }

        context.builder().build_return(None);

        {
            let dir = std::env::temp_dir();
            let file = [dir, "vrl.ll".into()]
                .iter()
                .collect::<std::path::PathBuf>();

            println!("LLVM IR -> {}", file.display());
            context.module.print_to_file(&file).unwrap();
        }

        if !context.function.verify(false) {
            return Err(format!(
                "Generated code for VRL function failed verification:\n{:?}",
                context.function
            ));
        }

        Ok(context)
    }
}

pub struct Context<'ctx> {
    context: &'ctx inkwell::context::Context,
    module: Module<'ctx>,
    builder: inkwell::builder::Builder<'ctx>,
    function: FunctionValue<'ctx>,
    execution_engine: ExecutionEngine<'ctx>,
    context_ref: PointerValue<'ctx>,
    result_ref: PointerValue<'ctx>,
    resolved_map: HashMap<Resolved, usize>,
    resolveds: Vec<GlobalValue<'ctx>>,
    ident_map: HashMap<Ident, usize>,
    idents: Vec<GlobalValue<'ctx>>,
    lookup_buf_map: HashMap<LookupBuf, usize>,
    lookup_bufs: Vec<GlobalValue<'ctx>>,
}

impl<'ctx> Context<'ctx> {
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

    pub fn into_const<T: Sized>(&self, value: T, name: &str) -> inkwell::values::GlobalValue<'ctx> {
        let size = std::mem::size_of::<T>();
        let global_type = self.context.i8_type().array_type(size as _);
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
            .map(|byte| self.context.i8_type().const_int(byte as _, false).into())
            .collect::<Vec<_>>();
        self.context.i8_type().const_array(array.as_slice())
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

        self.resolveds[index].as_pointer_value().into()
    }

    pub fn into_ident_const_ref(&mut self, ident: Ident) -> inkwell::values::PointerValue<'ctx> {
        let index = match self.ident_map.get(&ident) {
            Some(index) => *index,
            None => {
                let index = self.idents.len();
                let name = format!("{}", ident);
                let global = self.into_const(ident.clone(), &name);
                self.ident_map.insert(ident, index);
                self.idents.push(global);
                index
            }
        };

        self.idents[index].as_pointer_value().into()
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

        self.lookup_bufs[index].as_pointer_value().into()
    }

    pub fn build_alloca_resolved(
        &self,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let resolved_type_identifier =
            "std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>";
        let resolved_type = self
            .module
            .get_struct_type(resolved_type_identifier)
            .ok_or(format!(
                r#"failed getting type "{}" from module"#,
                resolved_type_identifier
            ))?;

        Ok(self.builder.build_alloca(resolved_type, name))
    }

    pub fn optimize(&self) {
        let pass_manager = PassManager::create(());

        pass_manager.add_argument_promotion_pass();
        pass_manager.add_constant_merge_pass();
        pass_manager.add_merge_functions_pass();
        pass_manager.add_dead_arg_elimination_pass();
        pass_manager.add_function_attrs_pass();
        pass_manager.add_function_inlining_pass();
        // pass_manager.add_always_inliner_pass();
        pass_manager.add_global_dce_pass();
        pass_manager.add_global_optimizer_pass();
        pass_manager.add_prune_eh_pass();
        pass_manager.add_ipsccp_pass();
        // pass_manager.add_internalize_pass(false);
        pass_manager.add_strip_dead_prototypes_pass();
        pass_manager.add_strip_symbol_pass();
        pass_manager.add_loop_vectorize_pass();
        pass_manager.add_slp_vectorize_pass();
        pass_manager.add_aggressive_dce_pass();
        pass_manager.add_bit_tracking_dce_pass();
        pass_manager.add_alignment_from_assumptions_pass();
        pass_manager.add_cfg_simplification_pass();
        pass_manager.add_dead_store_elimination_pass();
        pass_manager.add_scalarizer_pass();
        pass_manager.add_merged_load_store_motion_pass();
        pass_manager.add_gvn_pass();
        pass_manager.add_new_gvn_pass();
        pass_manager.add_ind_var_simplify_pass();
        pass_manager.add_instruction_combining_pass();
        pass_manager.add_jump_threading_pass();
        pass_manager.add_licm_pass();
        pass_manager.add_loop_deletion_pass();
        pass_manager.add_loop_idiom_pass();
        pass_manager.add_loop_rotate_pass();
        // pass_manager.add_loop_reroll_pass();
        pass_manager.add_loop_unroll_pass();
        pass_manager.add_loop_unswitch_pass();
        pass_manager.add_memcpy_optimize_pass();
        pass_manager.add_partially_inline_lib_calls_pass();
        pass_manager.add_lower_switch_pass();
        pass_manager.add_promote_memory_to_register_pass();
        pass_manager.add_reassociate_pass();
        pass_manager.add_sccp_pass();
        pass_manager.add_scalar_repl_aggregates_pass_ssa();
        pass_manager.add_simplify_lib_calls_pass();
        pass_manager.add_tail_call_elimination_pass();
        pass_manager.add_instruction_simplify_pass();
        pass_manager.add_correlated_value_propagation_pass();
        pass_manager.add_early_cse_mem_ssa_pass();
        pass_manager.add_lower_expect_intrinsic_pass();
        pass_manager.add_type_based_alias_analysis_pass();
        pass_manager.add_scoped_no_alias_aa_pass();
        pass_manager.add_basic_alias_analysis_pass();
        pass_manager.add_aggressive_inst_combiner_pass();
        pass_manager.add_loop_unroll_and_jam_pass();
        // pass_manager.add_coroutine_early_pass();
        // pass_manager.add_coroutine_split_pass();
        // pass_manager.add_coroutine_elide_pass();
        // pass_manager.add_coroutine_cleanup_pass();

        pass_manager.run_on(&self.module);

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
            // return Err("Generated code for VRL function failed verification".into());
        }
    }

    pub fn get_jit_function(
        &self,
    ) -> Result<
        JitFunction<'ctx, unsafe extern "C" fn(*mut crate::Context<'ctx>, *mut crate::Resolved)>,
        String,
    > {
        unsafe { self.execution_engine.get_function(VRL_EXECUTE_SYMBOL) }
            .map_err(|error| error.to_string())
    }
}
