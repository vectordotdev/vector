use crate::{expression::assignment::Target, Program, Value};
use inkwell::{
    builder::Builder,
    execution_engine::{ExecutionEngine, JitFunction},
    memory_buffer::MemoryBuffer,
    module::Module,
    passes::PassManager,
    values::{FunctionValue, GlobalValue, PointerValue},
    OptimizationLevel,
};
use std::collections::HashMap;

static VRL_EXECUTE_SYMBOL: &'static str = "vrl_execute";
static PRECOMPILED: &'static [u8] = include_bytes!("/Volumes/git/com.github/vectordotdev/vector/lib/vrl/compiler/src/precompiled/target/aarch64-apple-darwin/release/precompiled.bc");

// pub struct Llvm {
//     context: inkwell::context::Context,
// }

// impl Llvm {
//     pub fn new(program: &Program) -> Self {
//         Self {
//             context: inkwell::context::Context::create(),
//         }
//     }

//     pub fn compile<'a>(
//         &'a self,
//         program: &Program,
//     ) -> Result<
//         JitFunction<'a, unsafe extern "C" fn(*mut crate::Context<'a>, *mut crate::Resolved)>,
//         String,
//     > {
//         todo!();

//         Ok(vrl_execute)
//     }
// }

pub struct Context<'ctx> {
    context: &'ctx inkwell::context::Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    pass_manager: PassManager<Module<'ctx>>,
    function: FunctionValue<'ctx>,
    context_ref: PointerValue<'ctx>,
    result_ref: PointerValue<'ctx>,
    target_map: HashMap<Target, usize>,
    targets: Vec<GlobalValue<'ctx>>,
    value_map: HashMap<Value, usize>,
    values: Vec<GlobalValue<'ctx>>,
}

pub struct Token(inkwell::context::Context);

impl<'ctx> Context<'ctx> {
    pub fn create_token() -> Token {
        inkwell::targets::Target::initialize_native(
            &inkwell::targets::InitializationConfig::default(),
        )
        .expect("Failed to initialize native target");

        Token(inkwell::context::Context::create())
    }

    pub fn new(context: &'ctx Token) -> Result<Self, String> {
        // println!("new 1");
        let context = &context.0;
        let buffer = MemoryBuffer::create_from_memory_range(PRECOMPILED, "precompiled");
        let module = Module::parse_bitcode_from_buffer(&buffer, context)
            .map_err(|string| string.to_string())?;
        module.set_name("vrl");

        let builder = context.create_builder();

        let pass_manager = PassManager::create(());

        // println!("created pass manager");

        // pass_manager.add_function_inlining_pass();
        // pass_manager.add_instruction_combining_pass();
        // pass_manager.add_reassociate_pass();
        // pass_manager.add_gvn_pass();
        // pass_manager.add_cfg_simplification_pass();
        // pass_manager.add_basic_alias_analysis_pass();
        // pass_manager.add_promote_memory_to_register_pass();
        // pass_manager.add_instruction_combining_pass();
        // pass_manager.add_reassociate_pass();

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
        // pass_manager.add_scalar_repl_aggregates_pass();
        pass_manager.add_scalar_repl_aggregates_pass_ssa();
        pass_manager.add_simplify_lib_calls_pass();
        pass_manager.add_tail_call_elimination_pass();
        pass_manager.add_instruction_simplify_pass();
        // pass_manager.add_demote_memory_to_register_pass();
        pass_manager.add_verifier_pass();
        pass_manager.add_correlated_value_propagation_pass();
        // pass_manager.add_early_cse_pass();
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

        // pass_manager.initialize();

        // println!("initialized pass manager");

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

        // println!("new end");

        Ok(Self {
            context,
            module,
            builder,
            pass_manager,
            function,
            context_ref,
            result_ref,
            target_map: Default::default(),
            targets: Default::default(),
            value_map: Default::default(),
            values: Default::default(),
        })
    }

    pub fn compile(
        &mut self,
        program: &Program,
    ) -> Result<
        JitFunction<'ctx, unsafe extern "C" fn(*mut crate::Context<'ctx>, *mut crate::Resolved)>,
        String,
    > {
        // println!("compile 1");
        for expression in program.iter() {
            expression.emit_llvm(self)?;
        }

        self.builder().build_return(None);

        self.function().verify(true);

        // println!("compile 2");

        // self.function().print_to_stderr();

        // println!("optimize function");

        self.pass_manager.run_on(&self.module());

        // self.function().print_to_stderr();

        // println!("compile 3");

        let dir = std::env::temp_dir();
        let file = [dir, "vrl.ll".into()]
            .iter()
            .collect::<std::path::PathBuf>();

        println!("LLVM IR -> {}", file.display());

        // println!("compile 4");

        self.module().print_to_file(&file).unwrap();

        let execution_engine = self.create_execution_engine()?;

        // inkwell::support::load_library_permanently(None);

        println!("LLVM IR -> Machine Code");
        let vrl_execute = unsafe { execution_engine.get_function(VRL_EXECUTE_SYMBOL) }
            .map_err(|error| error.to_string())?;

        // println!("compile 5");

        Ok(vrl_execute)
    }

    pub fn create_execution_engine(&self) -> Result<ExecutionEngine<'ctx>, String> {
        self.module
            .create_jit_execution_engine(OptimizationLevel::Aggressive)
            .map_err(|string| string.to_string())
    }

    pub fn context(&self) -> &'ctx inkwell::context::Context {
        self.context
    }

    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    pub fn builder(&self) -> &Builder<'ctx> {
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

    pub fn into_target_const_ref(
        &mut self,
        target: Target,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let index = match self.target_map.get(&target) {
            Some(index) => *index,
            None => {
                let index = self.targets.len();
                let name = format!("{}", target);
                let global = self.into_const(target.clone(), &name);
                self.target_map.insert(target, index);
                self.targets.push(global);
                index
            }
        };

        Ok(self.targets[index].as_pointer_value().into())
    }

    pub fn into_value_const_ref(
        &mut self,
        value: Value,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let index = match self.value_map.get(&value) {
            Some(index) => *index,
            None => {
                let index = self.values.len();
                let name = format!("{}", value);
                let global = self.into_const(value.clone(), &name);
                self.value_map.insert(value, index);
                self.values.push(global);
                index
            }
        };

        Ok(self.targets[index].as_pointer_value().into())
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
        // Rust can't compute the size of generic arguments yet:
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

    pub fn build_alloca_resolved(
        &self,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let resolved_type_identifier = "core::result::Result<vrl_compiler::value::Value, vrl_compiler::expression::ExpressionError>";
        let resolved_type = self
            .module
            .get_struct_type(resolved_type_identifier)
            .ok_or(format!(
                r#"failed getting type "{}" from module"#,
                resolved_type_identifier
            ))?;

        Ok(self.builder.build_alloca(resolved_type, name))
    }
}
