use diagnostic::DiagnosticError;

use crate::{
    expression::{Expr, FunctionArgument},
    function::FunctionCompileContext,
    value::Kind,
    ExpressionError, Function, Parameter, Value,
};
use std::{any::Any, collections::BTreeMap};

pub enum VmArgument<'a> {
    Value(Value),
    Any(&'a Box<dyn Any + Send + Sync>),
}

impl<'a> VmArgument<'a> {
    fn into_value(self) -> Value {
        match self {
            VmArgument::Value(value) => value,
            _ => panic!(),
        }
    }

    fn into_any(self) -> &'a Box<dyn Any + Send + Sync> {
        match self {
            VmArgument::Any(any) => any,
            _ => panic!(),
        }
    }

    /// Returns the kind that this parameter is.
    /// If the parameter is an `Any`, we return `None` since the function that has created this parameter will
    /// have already done the required typechecking.
    fn kind(&self) -> Option<Kind> {
        match self {
            VmArgument::Value(value) => Some(value.into()),
            VmArgument::Any(_) => None,
        }
    }
}

pub struct VmArgumentList<'a> {
    args: &'static [Parameter],
    values: Vec<Option<VmArgument<'a>>>,
}

impl<'a> VmArgumentList<'a> {
    pub fn new(args: &'static [Parameter], values: Vec<Option<VmArgument<'a>>>) -> Self {
        Self { args, values }
    }

    fn argument_pos(&self, name: &str) -> usize {
        self.args
            .iter()
            .position(|param| param.keyword == name)
            .expect("parameter doesn't exist")
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn required(&mut self, name: &str) -> Value {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().unwrap().into_value()
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn optional(&mut self, name: &str) -> Option<Value> {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().map(|v| v.into_value())
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn required_any(&mut self, name: &str) -> &'a Box<dyn Any + Send + Sync> {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().unwrap().into_any()
    }

    /// Returns the parameter with the given name.
    /// Note that this can only be called once per parameter since the value is
    /// removed from the list.
    pub fn optional_any(&mut self, name: &str) -> Option<&'a Box<dyn Any + Send + Sync>> {
        // Get the position where the given argument is found in the parameter stack.
        let pos = self.argument_pos(name);

        // Return the parameter found at this position.
        self.values[pos].take().map(|v| v.into_any())
    }

    /// Validates the arguments are correct.
    pub fn check_arguments(&self) -> Result<(), ExpressionError> {
        for (param, args) in self.args.iter().zip(self.values.iter()) {
            match args.as_ref() {
                None if param.required => return Err("parameter is required".into()),
                Some(arg) if matches!(arg.kind(), Some(kind) if !param.kind().intersects(&kind)) => {
                    return Err(format!(
                        "expected {}, got {}",
                        param.kind(),
                        arg.kind().expect("argument has valid kind"),
                    )
                    .into());
                }
                _ => (),
            }
        }
        Ok(())
    }
}

/// Keeps clippy happy.
type CompiledArguments =
    Result<BTreeMap<&'static str, Box<dyn Any + Send + Sync>>, Box<dyn DiagnosticError>>;

/// Compiles the function arguments with the given argument list.
/// This is used by the stdlib unit tests.
pub fn function_compile_arguments<F>(
    function: &F,
    args: &crate::function::ArgumentList,
) -> CompiledArguments
where
    F: Function,
{
    // Clone to give us a mutable object.
    // Calling optional_value mutates the args object, which breaks things further on
    // in the tests if we don't work on our own copy.
    let mut args = args.clone();
    let mut result = BTreeMap::new();
    let context = FunctionCompileContext::new(Default::default());
    let params = function.parameters();
    let function_arguments: Vec<(&'static str, Option<FunctionArgument>)> = args.clone().into();

    for param in params {
        let arg = args
            .optional_value(param.keyword)
            .unwrap_or(None)
            .map(Expr::from);

        if let Some(arg) =
            function.compile_argument(&function_arguments, &context, param.keyword, arg.as_ref())?
        {
            result.insert(param.keyword, arg);
        }
    }

    Ok(result)
}

/// Compiles the arguments provided for the given function.
/// This needs to be a separate function to `compile_function_arguments` because the caller
/// needs to own the list of anys which has the same lifetime as the return.
/// This is used by the stdlib unit tests.
pub fn compile_arguments<'a, F>(
    function: &F,
    args: &mut crate::function::ArgumentList,
    anys: &'a BTreeMap<&'static str, Box<dyn Any + Send + Sync>>,
) -> VmArgumentList<'a>
where
    F: Function,
{
    let params = function.parameters();
    let values = params
        .iter()
        .map(|param| {
            anys.get(param.keyword).map(VmArgument::Any).or_else(|| {
                args.optional_value(param.keyword)
                    .expect("argument should be a literal")
                    .map(VmArgument::Value)
            })
        })
        .collect();

    VmArgumentList {
        args: params,
        values,
    }
}
