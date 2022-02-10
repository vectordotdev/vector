use crate::compiler::Compiler;
use crate::expression::assignment::Details;

use parser::ast;
use std::{fmt, sync::Arc};

use anymap::AnyMap;
use diagnostic::{DiagnosticError, Label, Note, Urls};

use crate::{
    expression,
    expression::{levenstein, ExpressionError, FunctionArgument, FunctionClosure, Noop},
    function::{ArgumentList, FunctionCompileContext, Parameter},
    parser::{Ident, Node},
    state::{ExternalEnv, LocalEnv},
    value::Kind,
    vm::OpCode,
    Context, Expression, Function, Resolved, Span, TypeDef,
};

#[derive(Clone)]
pub struct FunctionCall {
    abort_on_error: bool,
    expr: Box<dyn Expression>,
    maybe_fallible_arguments: bool,

    // used for enhancing runtime error messages (using abort-instruction).
    //
    // TODO: have span store line/col details to further improve this.
    span: Span,

    // used for equality check
    ident: &'static str,

    // The index of the function in the list of stdlib functions.
    // Used by the VM to identify this function when called.
    function_id: usize,
    arguments: Arc<Vec<Node<FunctionArgument>>>,
}

impl FunctionCall {
    #[allow(clippy::too_many_arguments)]
    pub fn new<'a>(
        call_span: Span,
        ident: Node<Ident>,
        abort_on_error: bool,
        arguments: Vec<Node<FunctionArgument>>,
<<<<<<< HEAD
        funcs: &[Box<dyn Function>],
        local: &mut LocalEnv,
        external: &mut ExternalEnv,
=======
        closure: Option<Node<ast::FunctionClosure>>,
        compiler: &'a mut Compiler,
>>>>>>> c6f603a5a (feat: initial iteration support spike)
    ) -> Result<Self, Error> {
        let (ident_span, ident) = ident.take();

        // Check if function exists.
        let (function_id, function) = match compiler
            .fns
            .iter()
            .enumerate()
            .find(|(_pos, f)| f.identifier() == ident.as_ref())
        {
            Some(function) => function,
            None => {
                let idents = compiler
                    .fns
                    .iter()
                    .map(|func| func.identifier())
                    .collect::<Vec<_>>();

                return Err(Error::Undefined {
                    ident_span,
                    ident: ident.clone(),
                    idents,
                });
            }
        };

        // Check function arity.
        if arguments.len() > function.parameters().len() {
            let arguments_span = {
                let start = arguments.first().unwrap().span().start();
                let end = arguments.last().unwrap().span().end();

                Span::new(start, end)
            };

            return Err(Error::WrongNumberOfArgs {
                arguments_span,
                max: function.parameters().len(),
            });
        }

        // Keeps track of positional argument indices.
        //
        // Used to map a positional argument to its keyword. Keyword arguments
        // can be used in any order, and don't count towards the index of
        // positional arguments.
        let mut index = 0;
        let mut list = ArgumentList::default();

        let mut maybe_fallible_arguments = false;
        for node in &arguments {
            let (argument_span, argument) = node.clone().take();

            let parameter = match argument.keyword() {
                // positional argument
                None => {
                    index += 1;
                    function.parameters().get(index - 1)
                }

                // keyword argument
                Some(k) => function
                    .parameters()
                    .iter()
                    .enumerate()
                    .find(|(_, param)| param.keyword == k)
                    .map(|(pos, param)| {
                        if pos == index {
                            index += 1;
                        }

                        param
                    }),
            }
            .ok_or_else(|| Error::UnknownKeyword {
                keyword_span: argument.keyword_span().expect("exists"),
                ident_span,
                keywords: function.parameters().iter().map(|p| p.keyword).collect(),
            })?;

            // Check if the argument is of the expected type.
<<<<<<< HEAD
            let argument_type_def = argument.type_def((local, external));
=======
            let argument_type_def = argument.type_def(compiler.state);
>>>>>>> c6f603a5a (feat: initial iteration support spike)
            let expr_kind = argument_type_def.kind();
            let param_kind = parameter.kind();

            if !param_kind.intersects(expr_kind) {
                return Err(Error::InvalidArgumentKind {
                    function_ident: function.identifier(),
                    abort_on_error,
                    arguments_fmt: arguments
                        .iter()
                        .map(|arg| arg.inner().to_string())
                        .collect::<Vec<_>>(),
                    parameter: *parameter,
                    got: expr_kind.clone(),
                    argument,
                    argument_span,
                });
            } else if !param_kind.is_superset(expr_kind) {
                maybe_fallible_arguments = true;
            }

            // Check if the argument is infallible.
            if argument_type_def.is_fallible() {
                return Err(Error::FallibleArgument {
                    expr_span: argument.span(),
                });
            }

            list.insert(parameter.keyword, argument.into_inner());
        }

        // Check missing required arguments.
        function
            .parameters()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.required)
            .filter(|(_, p)| !list.keywords().contains(&p.keyword))
            .try_for_each(|(i, p)| -> Result<_, _> {
                Err(Error::MissingArgument {
                    call_span,
                    keyword: p.keyword,
                    position: i,
                })
            })?;

<<<<<<< HEAD
        // We take the external context, and pass it to the function compile context, this allows
        // functions mutable access to external state, but keeps the internal compiler state behind
        // an immutable reference, to ensure compiler state correctness.
        let external_context = external.swap_external_context(AnyMap::new());

        let mut compile_ctx =
            FunctionCompileContext::new(call_span).with_external_context(external_context);

        let mut expr = function
            .compile((local, external), &mut compile_ctx, list)
=======
        // Check function closure validity.
        match (function.closure(), closure) {
            // Ensure function accepts closure.
            (None, Some(_)) => return Err(Error::UnexpectedClosure { call_span }),

            // Ensure required closure is present.
            (Some(_), None) => return Err(Error::MissingClosure { call_span }),

            // Check for invalid closure signature.
            (Some(definition), Some(closure)) => {
                // expand function closure, but don't expand the block yet.
                let ast::FunctionClosure { variables, block } = closure.into_inner();

                let mut matched = false;
                let mut found_type_def = None;
                let mut input_expression = None;
                for input in definition.inputs {
                    // Check type definition for linked parameter.
                    match list.arguments.get(input.parameter_keyword) {
                        // No argument provided for the given parameter keyword.
                        //
                        // This means the closure can't act on the input definition, so we continue
                        // on to the next. If no input definitions are valid, the closure is
                        // invalid.
                        None => continue,
                        Some(expr) => {
                            let type_def = expr.type_def(compiler.state);
                            // The type definition of the value does not match the expected closure
                            // type, continue to check if the closure eventually accepts this
                            // definition.
                            if !input.kind.is_superset(&type_def.kind()) {
                                found_type_def = Some(type_def.kind().clone());
                                input_expression = Some(expr);
                                continue;
                            }

                            matched = true;
                        }
                    };

                    // Now that we know we have a matching parameter argument with a valid type
                    // definition, we can move on to checking/defining the closure arguments.
                    //
                    // In doing so we:
                    //
                    // - check the arity of the closure arguments
                    // - set the expected type definition of each argument

                    if input.variables.len() != variables.len() {
                        return Err(Error::ClosureArityMismatch {
                            call_span,
                            expected: input.variables.len(),
                            supplied: variables.len(),
                        });
                    }

                    // Get the provided argument identifier in the same position as defined in the
                    // input definition.
                    //
                    // That is, if the function closure definition expects:
                    //
                    //   [bytes, integer]
                    //
                    // Then, given for an actual implementation of:
                    //
                    //   foo() -> { |bar, baz| }
                    //
                    // We set "bar" (index 0) to return bytes, and "baz" (index 1) to return an
                    // integer.
                    //
                    // An implementation with an arity mismatch will return in a compile-time
                    // error.
                    for (index, input_var) in input.variables.into_iter().enumerate() {
                        let call_ident = &variables[index];

                        // 1. get call_var type def from compiler state
                        // 2. compare against input_var.kind type def
                        // 3. if they don't match, error

                        // TODO:
                        // - need to register call_var type def as variable
                        // - how??

                        let details = Details {
                            type_def: input_var.kind.into(),
                            value: None,
                        };

                        compiler
                            .state
                            .insert_variable(call_ident.to_owned().into_inner(), details);
                    }
                }

                // None of the inputs matched the value type, this is a user error.
                if !matched {
                    return Err(Error::EnumerationTypeMismatch {
                        call_span,
                        found_kind: found_type_def.unwrap_or(Kind::any()),
                        input_expression: input_expression
                            .unwrap_or(&expression::Expr::Noop(Noop {}))
                            .clone(),
                    });
                }

                let block = compiler.compile_block(block);
                let closure = FunctionClosure::new(variables, block);
                list.set_closure(closure);
            }

            (None, None) => {}
        }

        let compile_ctx = FunctionCompileContext { span: call_span };

        let mut expr = function
            .compile(compiler.state, &compile_ctx, list)
>>>>>>> c6f603a5a (feat: initial iteration support spike)
            .map_err(|error| Error::Compilation { call_span, error })?;

        // Re-insert the external context into the compiler state.
        let _ = external.swap_external_context(compile_ctx.into_external_context());

        // Asking for an infallible function to abort on error makes no sense.
        // We consider this an error at compile-time, because it makes the
        // resulting program incorrectly convey this function call might fail.
        if abort_on_error
            && !maybe_fallible_arguments
<<<<<<< HEAD
            && !expr.type_def((local, external)).is_fallible()
=======
            && !expr.type_def(compiler.state).is_fallible()
>>>>>>> c6f603a5a (feat: initial iteration support spike)
        {
            return Err(Error::AbortInfallible {
                ident_span,
                abort_span: Span::new(ident_span.end(), ident_span.end() + 1),
            });
        }

        // Update the state if necessary.
<<<<<<< HEAD
        expr.update_state(local, external)
=======
        expr.update_state(compiler.state)
>>>>>>> c6f603a5a (feat: initial iteration support spike)
            .map_err(|err| Error::UpdateState {
                call_span,
                error: err.to_string(),
            })?;

        Ok(Self {
            abort_on_error,
            expr,
            maybe_fallible_arguments,
            span: call_span,
            ident: function.identifier(),
            function_id,
            arguments: Arc::new(arguments),
        })
    }

    /// Takes the arguments passed and resolves them into the order they are defined
    /// in the function
    /// The error path in this function should never really be hit as the compiler should
    /// catch these whilst creating the AST.
    fn resolve_arguments(
        &self,
        function: &(dyn Function),
    ) -> Result<Vec<(&'static str, Option<FunctionArgument>)>, String> {
        let params = function.parameters().to_vec();
        let mut result = params
            .iter()
            .map(|param| (param.keyword, None))
            .collect::<Vec<_>>();

        let mut unnamed = Vec::new();

        // Position all the named parameters, keeping track of all the unnamed for later.
        for param in self.arguments.iter() {
            match param.keyword() {
                None => unnamed.push(param.clone().take().1),
                Some(keyword) => {
                    match params.iter().position(|param| param.keyword == keyword) {
                        None => {
                            // The parameter was not found in the list.
                            return Err(format!("parameter {} not found.", keyword));
                        }
                        Some(pos) => {
                            result[pos].1 = Some(param.clone().take().1);
                        }
                    }
                }
            }
        }

        // Position all the remaining unnamed parameters
        let mut pos = 0;
        for param in unnamed {
            while result[pos].1.is_some() {
                pos += 1;
            }

            if pos > result.len() {
                return Err("Too many parameters".to_string());
            }

            result[pos].1 = Some(param);
        }

        Ok(result)
    }

    pub fn noop() -> Self {
        let expr = Box::new(Noop) as _;

        Self {
            abort_on_error: false,
            expr,
            maybe_fallible_arguments: false,
            span: Span::default(),
            ident: "noop",
            arguments: Arc::new(Vec::new()),
            function_id: 0,
        }
    }

    pub fn arguments_fmt(&self) -> Vec<String> {
        self.arguments
            .iter()
            .map(|arg| arg.inner().to_string())
            .collect::<Vec<_>>()
    }

    pub fn arguments_dbg(&self) -> Vec<String> {
        self.arguments
            .iter()
            .map(|arg| format!("{:?}", arg.inner()))
            .collect::<Vec<_>>()
    }
}

impl Expression for FunctionCall {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.expr.resolve(ctx).map_err(|err| match err {
            ExpressionError::Abort { .. } => {
                panic!("abort errors must only be defined by `abort` statement")
            }
            ExpressionError::Error {
                message,
                mut labels,
                notes,
            } => {
                labels.push(Label::primary(message.clone(), self.span));

                ExpressionError::Error {
                    message: format!(
                        r#"function call error for "{}" at ({}:{}): {}"#,
                        self.ident,
                        self.span.start(),
                        self.span.end(),
                        message
                    ),
                    labels,
                    notes,
                }
            }
        })
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let mut type_def = self.expr.type_def(state);

        // If one of the arguments only partially matches the function type
        // definition, then we mark the entire function as fallible.
        //
        // This allows for progressive type-checking, by handling any potential
        // type error the function throws, instead of having to enforce
        // exact-type invariants for individual arguments.
        //
        // That is, this program triggers the `InvalidArgumentKind` error:
        //
        //     slice(10, 1)
        //
        // This is because `slice` expects either a string or an array, but it
        // receives an integer. The concept of "progressive type checking" does
        // not apply in this case, because this call can never succeed.
        //
        // However, given these example events:
        //
        //     { "foo": "bar" }
        //     { "foo": 10.5 }
        //
        // If we were to run the same program, but against the `foo` field:
        //
        //     slice(.foo, 1)
        //
        // In this situation, progressive type checking _does_ make sense,
        // because we can't know at compile-time what the eventual value of
        // `.foo` will be. We mark `.foo` as "any", which includes the "array"
        // and "string" types, so the program can now be made infallible by
        // handling any potential type error the function returns:
        //
        //     slice(.foo, 1) ?? []
        //
        // Note that this rule doesn't just apply to "any" kind (in fact, "any"
        // isn't a kind, it's simply a term meaning "all possible VRL values"),
        // but it applies whenever there's an _intersection_ but not an exact
        // _match_ between two types.
        //
        // Here's another example to demonstrate this:
        //
        //     { "foo": "foobar" }
        //     { "foo": ["foo", "bar"] }
        //     { "foo": 10.5 }
        //
        //     foo = slice(.foo, 1) ?? .foo
        //     .foo = upcase(foo) ?? foo
        //
        // This would result in the following outcomes:
        //
        //     { "foo": "OOBAR" }
        //     { "foo": ["bar", "baz"] }
        //     { "foo": 10.5 }
        //
        // For the first event, both the `slice` and `upcase` functions succeed.
        // For the second event, only the `slice` function succeeds.
        // For the third event, both functions fail.
        //
        if self.maybe_fallible_arguments {
            type_def = type_def.with_fallibility(true);
        }

        if self.abort_on_error {
            type_def = type_def.with_fallibility(false);
        }

        type_def
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        (local, external): (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        // Resolve the arguments so they are in the order defined in the function.
        let args = match vm.function(self.function_id) {
            Some(fun) => self.resolve_arguments(fun)?,
            None => return Err(format!("Function {} not found.", self.function_id)),
        };

        // We take the external context, and pass it to the function compile context, this allows
        // functions mutable access to external state, but keeps the internal compiler state behind
        // an immutable reference, to ensure compiler state correctness.
        let external_context = external.swap_external_context(AnyMap::new());

        let mut compile_ctx =
            FunctionCompileContext::new(self.span).with_external_context(external_context);

        for (keyword, argument) in &args {
            let fun = vm.function(self.function_id).unwrap();
            let argument = argument.as_ref().map(|argument| argument.inner());

            // Call `compile_argument` for functions that need to perform any compile time processing
            // on the argument.
            match fun
                .compile_argument(&args, &mut compile_ctx, keyword, argument)
                .map_err(|err| err.to_string())?
            {
                Some(stat) => {
                    // The function has compiled this argument as a static.
                    let stat = vm.add_static(stat);
                    vm.write_opcode(OpCode::MoveStaticParameter);
                    vm.write_primitive(stat);
                }
                None => match argument {
                    Some(argument) => {
                        // Compile the argument, `MoveParameter` will move the result of the expression onto the
                        // parameter stack to be passed into the function.
                        argument.compile_to_vm(vm, (local, external))?;
                        vm.write_opcode(OpCode::MoveParameter);
                    }
                    None => {
                        // The parameter hasn't been specified, so just move an empty parameter onto the
                        // parameter stack.
                        vm.write_opcode(OpCode::EmptyParameter);
                    }
                },
            }
        }

        // Re-insert the external context into the compiler state.
        let _ = external.swap_external_context(compile_ctx.into_external_context());

        // Call the function with the given id.
        vm.write_opcode(OpCode::Call);
        vm.write_primitive(self.function_id);

        // We need to write the spans for error reporting.
        vm.write_primitive(self.span.start());
        vm.write_primitive(self.span.end());

        Ok(())
    }
}

impl fmt::Display for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ident.fmt(f)?;
        f.write_str("(")?;

        let arguments = self.arguments_fmt();
        let mut iter = arguments.iter().peekable();
        while let Some(arg) = iter.next() {
            f.write_str(arg)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str(")")
    }
}

impl fmt::Debug for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionCall(")?;
        self.ident.fmt(f)?;

        f.write_str("(")?;

        let arguments = self.arguments_dbg();
        let mut iter = arguments.iter().peekable();
        while let Some(arg) = iter.next() {
            f.write_str(arg)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str("))")
    }
}

impl PartialEq for FunctionCall {
    fn eq(&self, other: &Self) -> bool {
        self.ident == other.ident
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Error {
    #[error("call to undefined function")]
    Undefined {
        ident_span: Span,
        ident: Ident,
        idents: Vec<&'static str>,
    },

    #[error("wrong number of function arguments")]
    WrongNumberOfArgs { arguments_span: Span, max: usize },

    #[error("unknown function argument keyword")]
    UnknownKeyword {
        keyword_span: Span,
        ident_span: Span,
        keywords: Vec<&'static str>,
    },

    #[error("missing function argument")]
    MissingArgument {
        call_span: Span,
        keyword: &'static str,
        position: usize,
    },

    #[error("function compilation error: error[E{}] {}", error.code(), error)]
    Compilation {
        call_span: Span,
        error: Box<dyn DiagnosticError>,
    },

    #[error("can't abort infallible function")]
    AbortInfallible { ident_span: Span, abort_span: Span },

    #[error("invalid argument type")]
    InvalidArgumentKind {
        function_ident: &'static str,
        abort_on_error: bool,
        arguments_fmt: Vec<String>,
        parameter: Parameter,
        got: Kind,
        argument: FunctionArgument,
        argument_span: Span,
    },

    #[error("fallible argument")]
    FallibleArgument { expr_span: Span },

    #[error("error updating state {}", error)]
    UpdateState { call_span: Span, error: String },

    #[error("unexpected closure")]
    UnexpectedClosure { call_span: Span },

    #[error("missing closure")]
    MissingClosure { call_span: Span },

    #[error("invalid closure arity")]
    ClosureArityMismatch {
        call_span: Span,
        expected: usize,
        supplied: usize,
    },
    #[error("type mismatch in enumeration function call")]
    EnumerationTypeMismatch {
        call_span: Span,
        found_kind: Kind,
        input_expression: expression::Expr,
    },
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            Undefined { .. } => 105,
            WrongNumberOfArgs { .. } => 106,
            UnknownKeyword { .. } => 108,
            Compilation { .. } => 610,
            MissingArgument { .. } => 107,
            AbortInfallible { .. } => 620,
            InvalidArgumentKind { .. } => 110,
            FallibleArgument { .. } => 630,
            UpdateState { .. } => 640,
            UnexpectedClosure { .. } => 109,
            MissingClosure { .. } => 111,
            ClosureArityMismatch { .. } => 120,
            EnumerationTypeMismatch { .. } => 121,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            Undefined {
                ident_span,
                ident,
                idents,
            } => {
                let mut vec = vec![Label::primary("undefined function", ident_span)];
                let ident_chars = ident.as_ref().chars().collect::<Vec<_>>();

                if let Some((idx, _)) = idents
                    .iter()
                    .map(|possible| {
                        let possible_chars = possible.chars().collect::<Vec<_>>();
                        levenstein::distance(&ident_chars, &possible_chars)
                    })
                    .enumerate()
                    .min_by_key(|(_, score)| *score)
                {
                    {
                        let guessed: &str = idents[idx];
                        vec.push(Label::context(
                            format!(r#"did you mean "{}"?"#, guessed),
                            ident_span,
                        ));
                    }
                }

                vec
            }

            WrongNumberOfArgs {
                arguments_span,
                max,
            } => {
                let arg = if *max == 1 { "argument" } else { "arguments" };

                vec![
                    Label::primary("too many function arguments", arguments_span),
                    Label::context(
                        format!("this function takes a maximum of {} {}", max, arg),
                        arguments_span,
                    ),
                ]
            }

            UnknownKeyword {
                keyword_span,
                ident_span,
                keywords,
            } => vec![
                Label::primary("unknown keyword", keyword_span),
                Label::context(
                    format!(
                        "this function accepts the following keywords: {}",
                        keywords
                            .iter()
                            .map(|k| format!(r#""{}""#, k))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    ident_span,
                ),
            ],

            Compilation { call_span, error } => error
                .labels()
                .into_iter()
                .map(|mut label| {
                    label.span = *call_span;
                    label
                })
                .collect(),

            MissingArgument {
                call_span,
                keyword,
                position,
            } => {
                vec![Label::primary(
                    format!(
                        r#"required argument missing: "{}" (position {})"#,
                        keyword, position
                    ),
                    call_span,
                )]
            }

            AbortInfallible {
                ident_span,
                abort_span,
            } => {
                vec![
                    Label::primary("this function can't fail", ident_span),
                    Label::context("remove this abort-instruction", abort_span),
                ]
            }

            InvalidArgumentKind {
                parameter,
                got,
                argument,
                argument_span,
                ..
            } => {
                let keyword = parameter.keyword;
                let expected = parameter.kind();
                let expr_span = argument.span();

                // TODO: extract this out into a helper
                let kind_str = |kind: &Kind| {
                    if kind.is_any() {
                        kind.to_string()
                    } else if kind.is_exact() {
                        format!(r#"the exact type {}"#, kind)
                    } else {
                        format!("one of {}", kind)
                    }
                };

                vec![
                    Label::primary(
                        format!("this expression resolves to {}", kind_str(got)),
                        expr_span,
                    ),
                    Label::context(
                        format!(
                            r#"but the parameter "{}" expects {}"#,
                            keyword,
                            kind_str(&expected)
                        ),
                        argument_span,
                    ),
                ]
            }

            FallibleArgument { expr_span } => vec![
                Label::primary("this expression can fail", expr_span),
                Label::context(
                    "handle the error before passing it in as an argument",
                    expr_span,
                ),
            ],

            UpdateState { call_span, error } => vec![Label::primary(
                format!("an error occurred updating the compiler state: {}", error),
                call_span,
            )],
            UnexpectedClosure { call_span } => {
                vec![Label::primary("Unexpected Closure", call_span), Label::context("This function does not accept a closure.", call_span)]
            }
            MissingClosure { call_span } => vec![Label::primary("This function expects a closure and one was not provided.", call_span)], 
            ClosureArityMismatch { call_span, expected, supplied } => vec![Label::primary("This closure does not accept the required number of arguments.",call_span), Label::context(format!("{supplied} arguments were supplied, {expected} were expected."), call_span)],
            EnumerationTypeMismatch {
                call_span,
                found_kind,
                input_expression
            } => vec![Label::primary(
                format!(
                    "This input value does not have a known enumerable type."
                ),
                call_span,
            ), Label::context(format!("The expression {input_expression} has an inferred type of {found_kind} where an array or object was expected. If the type is known and is enumerable, try asserting the correct type with the array() or object() functions."), call_span)],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use Error::*;

        match self {
            WrongNumberOfArgs { .. } => vec![Note::SeeDocs(
                "function arguments".to_owned(),
                Urls::expression_docs_url("#arguments"),
            )],
            AbortInfallible { .. } | FallibleArgument { .. } => vec![Note::SeeErrorDocs],
            InvalidArgumentKind {
                function_ident,
                abort_on_error,
                arguments_fmt,
                parameter,
                argument,
                ..
            } => {
                // TODO: move this into a generic helper function
                let kind = parameter.kind();
                let guard = if kind.is_bytes() {
                    format!("string!({})", argument)
                } else if kind.is_integer() {
                    format!("int!({})", argument)
                } else if kind.is_float() {
                    format!("float!({})", argument)
                } else if kind.is_boolean() {
                    format!("bool!({})", argument)
                } else if kind.is_object() {
                    format!("object!({})", argument)
                } else if kind.is_array() {
                    format!("array!({})", argument)
                } else if kind.is_timestamp() {
                    format!("timestamp!({})", argument)
                } else {
                    return vec![];
                };

                let coerce = if kind.is_bytes() {
                    Some(format!(r#"to_string({}) ?? "default""#, argument))
                } else if kind.is_integer() {
                    Some(format!("to_int({}) ?? 0", argument))
                } else if kind.is_float() {
                    Some(format!("to_float({}) ?? 0", argument))
                } else if kind.is_boolean() {
                    Some(format!("to_bool({}) ?? false", argument))
                } else if kind.is_timestamp() {
                    Some(format!("to_timestamp({}) ?? now()", argument))
                } else {
                    None
                };

                let args = {
                    let mut args = String::new();
                    let mut iter = arguments_fmt.iter().peekable();
                    while let Some(arg) = iter.next() {
                        args.push_str(arg);
                        if iter.peek().is_some() {
                            args.push_str(", ");
                        }
                    }

                    args
                };

                let abort = if *abort_on_error { "!" } else { "" };

                let mut notes = vec![];

                let call = format!("{}{}({})", function_ident, abort, args);

                notes.append(&mut Note::solution(
                    "ensuring an appropriate type at runtime",
                    vec![format!("{} = {}", argument, guard), call.clone()],
                ));

                if let Some(coerce) = coerce {
                    notes.append(&mut Note::solution(
                        "coercing to an appropriate type and specifying a default value as a fallback in case coercion fails",
                        vec![format!("{} = {}", argument, coerce), call],
                    ))
                }

                notes.push(Note::SeeErrorDocs);

                notes
            }

            Compilation { error, .. } => error.notes(),

            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        expression::{Expr, Literal},
        state::ExternalEnv,
        value::kind,
    };

    use super::*;

    #[derive(Clone, Debug)]
    struct Fn;

    impl Expression for Fn {
        fn resolve(&self, _ctx: &mut Context) -> Resolved {
            todo!()
        }

        fn type_def(&self, _state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
            TypeDef::null().infallible()
        }
    }

    #[derive(Debug)]
    struct TestFn;

    impl Function for TestFn {
        fn identifier(&self) -> &'static str {
            "test"
        }

        fn examples(&self) -> &'static [crate::function::Example] {
            &[]
        }

        fn parameters(&self) -> &'static [Parameter] {
            &[
                Parameter {
                    keyword: "one",
                    kind: kind::INTEGER,
                    required: false,
                },
                Parameter {
                    keyword: "two",
                    kind: kind::INTEGER,
                    required: false,
                },
                Parameter {
                    keyword: "three",
                    kind: kind::INTEGER,
                    required: false,
                },
            ]
        }

        fn compile(
            &self,
            _state: (&mut LocalEnv, &mut ExternalEnv),
            _ctx: &mut FunctionCompileContext,
            _arguments: ArgumentList,
        ) -> crate::function::Compiled {
            Ok(Box::new(Fn))
        }

        fn call_by_vm(
            &self,
            _ctx: &mut Context,
            _args: &mut crate::vm::VmArgumentList,
        ) -> Result<value::Value, ExpressionError> {
            unimplemented!()
        }
    }

    fn create_node<T>(inner: T) -> Node<T> {
        Node::new(Span::new(0, 0), inner)
    }

    fn create_argument(ident: Option<&str>, value: i64) -> FunctionArgument {
        FunctionArgument::new(
            ident.map(|ident| create_node(Ident::new(ident))),
            create_node(Expr::Literal(Literal::Integer(value))),
        )
    }

    fn create_function_call(arguments: Vec<Node<FunctionArgument>>) -> FunctionCall {
        let mut local = LocalEnv::default();
        let mut external = ExternalEnv::default();

        FunctionCall::new(
            Span::new(0, 0),
            Node::new(Span::new(0, 0), Ident::new("test")),
            false,
            arguments,
            &[Box::new(TestFn) as _],
            &mut local,
            &mut external,
        )
        .unwrap()
    }

    #[test]
    fn resolve_arguments_simple() {
        let call = create_function_call(vec![
            create_node(create_argument(None, 1)),
            create_node(create_argument(None, 2)),
            create_node(create_argument(None, 3)),
        ]);

        let params = call.resolve_arguments(&TestFn);
        let expected: Vec<(&'static str, Option<FunctionArgument>)> = vec![
            ("one", Some(create_argument(None, 1))),
            ("two", Some(create_argument(None, 2))),
            ("three", Some(create_argument(None, 3))),
        ];

        assert_eq!(Ok(expected), params);
    }

    #[test]
    fn resolve_arguments_named() {
        let call = create_function_call(vec![
            create_node(create_argument(Some("one"), 1)),
            create_node(create_argument(Some("two"), 2)),
            create_node(create_argument(Some("three"), 3)),
        ]);

        let params = call.resolve_arguments(&TestFn);
        let expected: Vec<(&'static str, Option<FunctionArgument>)> = vec![
            ("one", Some(create_argument(Some("one"), 1))),
            ("two", Some(create_argument(Some("two"), 2))),
            ("three", Some(create_argument(Some("three"), 3))),
        ];

        assert_eq!(Ok(expected), params);
    }

    #[test]
    fn resolve_arguments_named_unordered() {
        let call = create_function_call(vec![
            create_node(create_argument(Some("three"), 3)),
            create_node(create_argument(Some("two"), 2)),
            create_node(create_argument(Some("one"), 1)),
        ]);

        let params = call.resolve_arguments(&TestFn);
        let expected: Vec<(&'static str, Option<FunctionArgument>)> = vec![
            ("one", Some(create_argument(Some("one"), 1))),
            ("two", Some(create_argument(Some("two"), 2))),
            ("three", Some(create_argument(Some("three"), 3))),
        ];

        assert_eq!(Ok(expected), params);
    }

    #[test]
    fn resolve_arguments_unnamed_unordered_one() {
        let call = create_function_call(vec![
            create_node(create_argument(Some("three"), 3)),
            create_node(create_argument(None, 2)),
            create_node(create_argument(Some("one"), 1)),
        ]);

        let params = call.resolve_arguments(&TestFn);
        let expected: Vec<(&'static str, Option<FunctionArgument>)> = vec![
            ("one", Some(create_argument(Some("one"), 1))),
            ("two", Some(create_argument(None, 2))),
            ("three", Some(create_argument(Some("three"), 3))),
        ];

        assert_eq!(Ok(expected), params);
    }

    #[test]
    fn resolve_arguments_unnamed_unordered_two() {
        let call = create_function_call(vec![
            create_node(create_argument(Some("three"), 3)),
            create_node(create_argument(None, 1)),
            create_node(create_argument(None, 2)),
        ]);

        let params = call.resolve_arguments(&TestFn);
        let expected: Vec<(&'static str, Option<FunctionArgument>)> = vec![
            ("one", Some(create_argument(None, 1))),
            ("two", Some(create_argument(None, 2))),
            ("three", Some(create_argument(Some("three"), 3))),
        ];

        assert_eq!(Ok(expected), params);
    }
}
