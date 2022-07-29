use inkwell::{
    builder::Builder,
    module::Module,
    values::{BasicMetadataValueEnum, CallSiteValue, FunctionValue},
};

#[derive(Debug, Clone, Copy)]
pub struct PrecompiledFunction<'ctx, const N: usize> {
    pub function: FunctionValue<'ctx>,
}

impl<'ctx> PrecompiledFunction<'ctx, 1> {
    pub fn build_call(
        &self,
        builder: &Builder<'ctx>,
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
        builder: &Builder<'ctx>,
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
        builder: &Builder<'ctx>,
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
        builder: &Builder<'ctx>,
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

include!(concat!(env!("OUT_DIR"), "/signatures.rs"));
