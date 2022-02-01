# RFC 10517 - 2021-12-20 - LLVM Backend for VRL

Performance is a key aspect of VRL. We aim to provide a language that is
["extremely fast and efficient"](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/website/cue/reference/remap/features/compilation.cue#L6)
and
["ergonomically safe in that it makes it difficult to create slow or buggy VRL programs"](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/website/cue/reference/remap/features/ergonomic_safety.cue#L4).
Moving towards this goal, we propose to speed up general program execution by
using LLVM to eliminate any runtime overhead that is currently associated with
interpreting a VRL program.

## Common Misconceptions

Below we present a list of statements that have commonly come up when discussing
the current execution model of VRL. We illustrate where these thoughts come from
and how their understanding is often counter-intuitive.

### "VRL programs are compiled to and run as native Rust code" [↪](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/website/cue/reference/remap/features/compilation.cue#L4)

Vector's codebase consists entirely of code written in Rust, so one might be
inclined to conclude that anything running inside of Vector is running as
"native Rust" code. This is true from a computability point of view - VRL
processes events in a way that is semantically indistinguishable from
transformations that were hand-written in Rust. However, implementation details
are critical for execution time, not just the semantic definition of their
computation.

In particular, removing the hidden indirection of "we implement a mechanism that
can perform sufficiently general computation within our program to execute
program logic" to "we implement a program that executes program logic" makes a
surprisingly large
[difference](#the-time-spent-within-the-vrl-interpretervm-itself-is-small-removing-this-overhead-can-hardly-result-in-significant-performance-improvements),
even if the aforementioned mechanism is written in a high-performance language.

In its current implementation, "VRL programs are compiled to a representation
that is interpreted in native Rust code" would be a more fitting description.

### "VRL programs are extremely fast and efficient, with performance characteristics very close to Rust itself" [↪](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/website/cue/reference/remap/features/compilation.cue#L6)

Many code paths taken during execution of a VRL program have been compiled by
Rust, e.g. when inserting or deleting paths of a VRL value, parsing JSON or
matching with regular expressions. These paths are highly optimized and don't
incur any runtime overhead on top of what the Rust compiler is able to produce.

However, the top-level control flow of a VRL program is orchestrated at runtime
and therefore different from a semantically equivalent transformation that has
been implemented in Rust. The main difference lies in that when compiling a Rust
program, the CPU knows statically which branches are taken between VRL
expressions (minus conditionals and error handling). We elaborate on the
**super-proportional** effects this has on performance further
[below](#the-time-spent-within-the-vrl-interpretervm-itself-is-small-removing-this-overhead-can-hardly-result-in-significant-performance-improvements).

### "VRL has no runtime [...]" [↪](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/website/cue/reference/remap/features/compilation.cue#L7)

There is no way to point the CPU instruction counter to a VRL
[`Program`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/program.rs#L5-L10)
to execute it. Instead, it relies on a runtime to interpret a VRL program by
implementing a
[`resolve`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/expression.rs#L51-L56)
method for each expression. It fits the very definition of
["_any behavior not directly attributable to the program itself_"](https://en.wikipedia.org/wiki/Runtime_system#Overview)
well and even exists as
[`Runtime`](https://github.com/vectordotdev/vector/blob/80268ee9b66def9e8cba848b29371013b7cd8b9c/lib/vrl/core/src/runtime.rs#L11-L15)
in our code.

### "The time spent within the VRL interpreter/VM itself is small, removing this overhead can hardly result in significant performance improvements"

When inspecting flamegraphs of VRL program execution one can see that, on a very
roughly estimate, no more than 25% of the time is spent in the interpret call
itself without progressing the program state. So, how can one expect that by
removing this overhead, any performance increase bigger than 33% would be
attainable?

The answer has largely to do with the
[memory bottleneck in the von Neumann architecture](https://en.wikipedia.org/wiki/Von_Neumann_architecture#Von_Neumann_bottleneck)
and the mechanisms modern CPU architecture employ to mitigate it.

The CPU can improve execution speed of subsequent CPU instructions by using
[instruction pipelining](https://en.wikipedia.org/wiki/Instruction_pipelining)
as long as these instructions are not fragmented between unpredictable paths.
When conditional branches exist but are heavily biased, the CPU can
[speculatively execute](https://en.wikipedia.org/wiki/Branch_predictor)
instructions and read/write from main memory to hide the
[latency of memory access](https://en.wikipedia.org/wiki/Memory_hierarchy#Examples),
which is _orders of magnitude_ higher than accessing CPU registers/caches or
executing arithmetic operations.

In the current execution model, the CPU is not able to predict any control flow
on the boundary between any VRL (sub)expression, majorly limiting the CPU
utilization by [stalling](https://en.wikipedia.org/wiki/CPU_cache#CPU_stalls)
the CPU.

### "Optimizing for single core performance is not as important when one can resort to parallelism first"

When a problem looks
[embarrassingly parallel](https://en.wikipedia.org/wiki/Embarrassingly_parallel)
one might think that squeezing out single-core performance does not look very
worthwhile when adding more threads would seemingly always have an outsized
effect.

However, even a small amount of synchronization points can have a detrimental
impact on optimal performance. According to
[Amdahl's law](https://en.wikipedia.org/wiki/Amdahl%27s_law), even when only 5%
of a program can not be parallelized, the maximum performance increase with an
_infinite_ amount of threads is capped at 20x.

### "LLVM is a virtual machine"

Judging from its name, one might assume that LL**VM** stands for "... virtual
machine"[^1] and that it's merely a more general and sophisticated VM
implementation than our upcoming special purpose VRL virtual machine.

However, even though LLVM provides a virtual instruction set architecture, it is
an intermediate representation that exists only during the compilation process
between high-level language and machine code **without** any interpretation at
runtime.

One essential part of LLVM are optimization passes that act on LLVM IR, e.g. by
inlining functions, merging code branches, promoting memory access to register
access, performing constant-folding, batching allocations and more.

Rust uses LLVM to emit machine code, and we intend to employ the exact same
technique.

## Context / Cross cutting concerns

There's ongoing work on implementing a VM for VRL:
[#10011](https://github.com/vectordotdev/vector/pull/10011). While it reduces
the interpretation overhead over the current expression traversal, it doesn't
eliminate the overhead entirely. More importantly, it doesn't fundamentally
improve behavior for speculative execution / branch prediction, since the CPU
can't predict the next instruction in the interpreter loop.

## Scope

### In scope

Introducing a new execution model to VRL that directly executes machine code
without runtime interpretation overhead, which can be opt in by users.

### Out of scope

Overall, this is an experiment to gauge how much performance improvements can be
won by using LLVM. We will not roll out the new backend for production use yet
and need to investigate the specific security and performance needs of our users
going forward.

Any optimization that applies to all execution models for VRL (traversal, VM and
LLVM) is not interesting for this consideration, e.g. improving access paths to
VRL
[`Value`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/value.rs#L21-L32)s.

## Pain

Performance investigations of various Vector topologies suggested that the
single-core performance of VRL is a bottleneck in many cases.

## Proposal

### User Experience

The semantics of VRL stay **unchanged**. Any case where a VRL program is not
strictly faster in the LLVM execution model versus traversal or VM is considered
a definite bug.

This is an unconditional win for user experience.

### Introduction to LLVM

To get familiar with how LLVM looks like and how its code generation builder
works, I recommend reading through the official tutorial
["Kaleidoscope: Code generation to LLVM IR"](https://llvm.org/docs/tutorial/MyFirstLanguageFrontend/LangImpl03.html).

There exist an adapted version of the
[LLVM Kaleidoscope tutorial in Rust](https://github.com/TheDan64/inkwell/blob/master/examples/kaleidoscope/main.rs)
for [`inkwell`](https://github.com/TheDan64/inkwell), a crate that exposes a
safe wrapper around [LLVM's C API](https://llvm.org/doxygen/group__LLVMC.html).

Another great reference is Mukul Rathi's
["A Complete Guide to LLVM for Programming Language Creators"](https://mukulrathi.com/create-your-own-programming-language/llvm-ir-cpp-api-tutorial/).

Godbolt's [Compiler Explorer](https://godbolt.org/) is a great way to understand
how compilers emit LLVM. E.g. running

```rust
#[no_mangle]
pub extern "C" fn foo(n: i32) -> i32 {
    n * 42
}
```

through the compiler and setting the `rustc` argument to `--emit=llvm-ir -O`
emits

```llvm
define i32 @foo(i32 %n) unnamed_addr #0 !dbg !6 {
  %0 = mul i32 %n, 42, !dbg !10
  ret i32 %0, !dbg !11
}
```

Running `rustc ./program.rs --crate-type=lib --emit=llvm-ir -O` locally will
accomplish the same.

### Implementation

On a high level, the goal is to produce executable machine code for a VRL
program via emitting LLVM IR. When Vector launches, the VRL program is parsed,
translated to LLVM IR, compiled to machine code via LLVM and dynamically loaded
into the running process. The resulting `vrl_execute` function symbol is then
resolved from the binary and called for each event to be transformed.

Instead of recursively calling
[`resolve`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/expression.rs#L51-L56)
on an
[`Expression`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/expression.rs#L50),
we add an `emit_llvm` method to the trait:

```rust
/// Emit LLVM IR that computes the `Value` for this expression.
fn emit_llvm<'ctx>(
    &self,
    state: &crate::state::Compiler,
    context: &mut crate::llvm::Context<'ctx>,
) -> Result<(), String>;
```

where `Context` is defined as

```rust
pub struct Context<'ctx> {
    context: &'ctx inkwell::context::Context,
    execution_engine: inkwell::execution_engine::ExecutionEngine<'ctx>,
    module: inkwell::module::Module<'ctx>,
    builder: inkwell::builder::Builder<'ctx>,
    function: inkwelll::values::FunctionValue<'ctx>,
    context_ref: inkwelll::values::PointerValue<'ctx>,
    result_ref: inkwelll::values::PointerValue<'ctx>,
    ...
}
```

We will preserve the existing `resolve` methods for the time being since they
provide a great reference for the current semantics of VRL and can serve as a
target for automated correctness tests.

By convention, each expression can call `context.result_ref()` to get an LLVM
`PointerValue` with a pointer to where the
[`Resolved`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/expression.rs#L48)
value should be stored.

The result pointer can be temporarily changed using `context.set_result_ref()`.
This mechanism allows the parent expression to call `emit_llvm` on the child
while controlling where the machine code emitted for the child expression stores
the result. This is useful, e.g. when emitting a binary operation where both of
its operands need to be computed first.

Calling `context.context_ref()` returns a reference to the VRL
[`Context`](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/lib/vrl/compiler/src/context.rs#L5-L9)
that is provided by any Vector component that uses VRL internally.

For anything less trivial than emitting branches or calling functions, we want
to leverage the Rust compiler. For one, this allows us to not concern ourselves
with memory layout and doesn't force us to define an FFI when we only use basic
integer types or pointers/references. It also provides us with Rust's memory
safety guarantees for large parts of the emitted LLVM IR.

Specifically, the idea is to compose VRLs functionality entirely via the
following LLVM instructions only:

- `alloca` stack allocations
- `br` conditional and unconditional branching
- `call`s to Rust stubs
- `global`s for constants

This makes the implementation quite maintainable to Rust programmers, even with
only a superficial understanding of LLVM and the emitted LLVM IR can be
sufficiently optimized by LLVM's optimization passes.

Most expressions which rely predominantly on precompiled Rust build LLVM IR
similar to the following

```rust
let fn_ident = "vrl_resolved_initialize";
let fn_impl = ctx
    .module()
    .get_function(fn_ident)
    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
ctx.builder()
    .build_call(fn_impl, &[result_ref.into()], fn_ident);
```

which will emit the LLVM instruction

```llvm
call void @vrl_resolved_initialize(%"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
```

This call refers to a function compiled by Rust in the same LLVM module.
However, we don't need to actually call this function, the implementation can be
inlined and optimized by LLVM since the whole source code is known. This way of
emitting LLVM IR is merely very convenient to stitch together code fragments.

For temporary values needed e.g. in binary operations we can allocate
uninitialized stack values:

```rust
let result_temp_ref = ctx.build_alloca_resolved("temp")?;
```

which will emit the LLVM instruction

```llvm
%temp = alloca %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>", align 8
```

it is our responsibility to initialize and drop the value accordingly. This can
be accomplished by calling the implementations for `vrl_resolved_initialize` and
`vrl_resolved_drop` as shown further below. To facilitate safe usage, we can
expose a module builder API where allocating a temporary value immediately
inserts a call to `vrl_resolved_initialize` and to `vrl_resolved_drop` when the
builder value is dropped from the scope. In combination with the
[`llvm.lifetime.start`](https://llvm.org/docs/LangRef.html#llvm-lifetime-start-intrinsic)
and
[`llvm.lifetime.end`](https://llvm.org/docs/LangRef.html#llvm-lifetime-end-intrinsic)
intrinsics, that should guard us against use of uninitialized values or usage
after the value has been dropped.

VRL constants can be moved into the LLVM module by consuming the constant value
of type `T` on the Rust side and transmuting it to `[i8]` written to a LLVM
global. This is safe since Rust's semantics allow all types to be moved in
memory unless they are `Pin`. Writing constants into the LLVM module has the
benefit of allowing LLVM to apply constant folding at compile time. To guarantee
that the resources that may have been allocated on the Rust side for creating
the VRL constant are cleaned up properly, we transmute it back to `T` when
unloading the LLVM module and drop it afterwards accordingly.

Below we show a preliminary, work-in-progress excerpt of the precompiled
functions. The LLVM module will be initialized with the resulting bitcode.
Therefore, these function symbols possibly do not exist at runtime anymore if
they are optimized out by LLVM.

```rust
#[no_mangle]
pub extern "C" fn vrl_resolved_initialize(result: *mut Resolved) {
    unsafe { result.write(Ok(Value::Null)) };
}

#[no_mangle]
pub extern "C" fn vrl_resolved_drop(result: *mut Resolved) {
    drop(unsafe { result.read() });
}

#[no_mangle]
pub extern "C" fn vrl_resolved_is_err(result: &mut Resolved) -> bool {
    result.is_err()
}

#[no_mangle]
pub extern "C" fn vrl_resolved_boolean_is_true(result: &Resolved) -> bool {
    result.as_ref().unwrap().as_boolean().unwrap()
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_external_impl(
    ctx: &mut Context,
    path: &LookupBuf,
    resolved: &Resolved,
) {
    let value = resolved.as_ref().unwrap().clone();
    let _ = ctx.target_mut().insert(path, value);
}

#[no_mangle]
pub extern "C" fn vrl_expression_literal_impl(value: &Value, result: &mut Resolved) {
    *result = Ok(value.clone());
}

#[no_mangle]
pub extern "C" fn vrl_expression_op_eq_impl(rhs: &mut Resolved, result: &mut Resolved) {
    let rhs = std::mem::replace(rhs, Ok(Value::Null));
    *result = match (result.clone(), rhs) {
        (Ok(lhs), Ok(rhs)) => Ok(Value::Boolean(rhs == lhs)),
        _ => unimplemented!(),
    };
}

#[no_mangle]
pub extern "C" fn vrl_expression_query_target_external_impl(
    context: &mut Context,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    *result = Ok(context
        .target()
        .get(path)
        .ok()
        .flatten()
        .unwrap_or(Value::Null));
}
```

With the precompiled library, we can emit code in terms of it by utilizing stack
allocations, branches and functions calls only. E.g. the LLVM IR for the
following VRL program:

```vrl
if .status == 123 {
    .foo = "bar"
}
```

Would look like this:

```llvm
; Function Attrs: mustprogress nofree norecurse nosync nounwind readnone uwtable willreturn
define void @vrl_execute(%"vrl_compiler::Context"* noalias nocapture align 8 dereferenceable(32) %context, %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* noalias nocapture align 8 dereferenceable(88) %result) unnamed_addr #55 {
start:
  br label %if_statement_begin

if_statement_begin:                               ; preds = %start
  br label %"op_==_begin"

"op_==_begin":                                    ; preds = %if_statement_begin
  call void @vrl_expression_query_target_external_impl(%"vrl_compiler::Context"* %context, %"lookup_buf::LookupBuf"* bitcast ([32 x i8]* @status to %"lookup_buf::LookupBuf"*), %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
  %rhs = alloca %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>", align 8
  call void @vrl_resolved_initialize(%"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %rhs)
  br label %literal_begin

literal_begin:                                    ; preds = %"op_==_begin"
  call void @vrl_expression_literal_impl(%"memmem::SearcherKind"* bitcast ([40 x i8]* @"123" to %"memmem::SearcherKind"*), %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %rhs)
  call void @vrl_expression_op_eq_impl(%"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %rhs, %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
  call void @vrl_resolved_drop(%"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %rhs)
  %vrl_resolved_boolean_is_true = call i1 @vrl_resolved_boolean_is_true(%"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
  br i1 %vrl_resolved_boolean_is_true, label %if_statement_if_branch, label %if_statement_else_branch

if_statement_end:                                 ; preds = %if_statement_else_branch, %block_end
  ret void

if_statement_if_branch:                           ; preds = %literal_begin
  br label %block_begin

if_statement_else_branch:                         ; preds = %literal_begin
  br label %if_statement_end

block_begin:                                      ; preds = %if_statement_if_branch
  br label %assignment_single_begin

block_end:                                        ; preds = %block_next, %block_error
  br label %if_statement_end

block_error:                                      ; preds = %assignment_single_end
  br label %block_end

assignment_single_begin:                          ; preds = %block_begin
  br label %literal_begin1

assignment_single_end:                            ; preds = %literal_begin1
  %vrl_resolved_is_err = call i1 @vrl_resolved_is_err(%"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
  br i1 %vrl_resolved_is_err, label %block_error, label %block_next

literal_begin1:                                   ; preds = %assignment_single_begin
  call void @vrl_expression_literal_impl(%"memmem::SearcherKind"* bitcast ([40 x i8]* @"\22bar\22" to %"memmem::SearcherKind"*), %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
  call void @vrl_expression_assignment_target_insert_external_impl(%"vrl_compiler::Context"* %context, %"lookup_buf::LookupBuf"* bitcast ([32 x i8]* @foo to %"lookup_buf::LookupBuf"*), %"std::result::Result<vrl_compiler::Value, vrl_compiler::ExpressionError>"* %result)
  br label %assignment_single_end

block_next:                                       ; preds = %assignment_single_end
  br label %block_end
}
```

After running several LLVM optimization passes over the LLVM IR:

```llvm
; Function Attrs: nofree norecurse nosync nounwind readnone uwtable willreturn
define void @vrl_execute(%142* noalias nocapture align 8 dereferenceable(32) %0, %752* noalias nocapture align 8 dereferenceable(88) %1) unnamed_addr #87 personality i32 (i32, i32, i64, %462*, %9*)* @rust_eh_personality {
  %3 = alloca %529*, align 8
  %4 = alloca %752, align 8
  %5 = alloca [5 x i64], align 8
  %6 = alloca %135, align 8
  %7 = alloca %116, align 8
  %8 = alloca %135, align 8
  tail call void @vrl_expression_query_target_external_impl(%142* nonnull %0, %74* bitcast ([32 x i8]* @16146 to %74*), %752* nonnull %1) #104
  %9 = alloca %752, align 8
  %10 = getelementptr inbounds %752, %752* %9, i64 0, i32 0
  store i64 0, i64* %10, align 8
  %11 = getelementptr inbounds %752, %752* %9, i64 0, i32 1
  %12 = bitcast [10 x i64]* %11 to i8*
  store i8 8, i8* %12, align 8
  tail call void @llvm.experimental.noalias.scope.decl(metadata !99596)
  %13 = bitcast [5 x i64]* %5 to i8*
  call void @llvm.lifetime.start.p0i8(i64 40, i8* nonnull %13), !noalias !99596
  %14 = bitcast [5 x i64]* %5 to %135*
  call fastcc void @17203(%135* noalias nocapture nonnull dereferenceable(40) %14, %135* nonnull align 8 dereferenceable(40) bitcast ([40 x i8]* @16147 to %135*)) #104, !noalias !99596
  %15 = bitcast [10 x i64]* %11 to %135*
  invoke fastcc void @17183(%135* nonnull %15)
          to label %18 unwind label %16

common.resume:                                    ; preds = %49, %16
  %common.resume.op = phi { i8*, i32 } [ %17, %16 ], [ %50, %49 ]
  resume { i8*, i32 } %common.resume.op

16:                                               ; preds = %2
  %17 = landingpad { i8*, i32 }
          cleanup
  store i64 0, i64* %10, align 8, !alias.scope !99596
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* noundef nonnull align 8 dereferenceable(40) %12, i8* noundef nonnull align 8 dereferenceable(40) %13, i64 40, i1 false)
  br label %common.resume

18:                                               ; preds = %2
  store i64 0, i64* %10, align 8, !alias.scope !99596
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* noundef nonnull align 8 dereferenceable(40) %12, i8* noundef nonnull align 8 dereferenceable(40) %13, i64 40, i1 false)
  call void @llvm.lifetime.end.p0i8(i64 40, i8* nonnull %13), !noalias !99596
  call void @vrl_expression_op_eq_impl(%752* nonnull %9, %752* nonnull %1) #104
  %19 = bitcast %752* %4 to i8*
  call void @llvm.lifetime.start.p0i8(i64 88, i8* nonnull %19)
  %20 = bitcast %752* %9 to i8*
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* noundef nonnull align 8 dereferenceable(88) %19, i8* noundef nonnull align 8 dereferenceable(88) %20, i64 88, i1 false) #104
  %21 = getelementptr inbounds %752, %752* %4, i64 0, i32 0
  %22 = load i64, i64* %21, align 8, !range !220, !alias.scope !99599
  %23 = icmp eq i64 %22, 0
  %24 = getelementptr inbounds %752, %752* %4, i64 0, i32 1
  br i1 %23, label %25, label %27

25:                                               ; preds = %18
  %26 = bitcast [10 x i64]* %24 to %135*
  call fastcc void @17183(%135* nonnull %26) #104
  br label %29

27:                                               ; preds = %18
  %28 = bitcast [10 x i64]* %24 to %529*
  call void @17184(%529* nonnull %28)
  br label %29

29:                                               ; preds = %25, %27
  call void @llvm.lifetime.end.p0i8(i64 88, i8* nonnull %19)
  %30 = getelementptr %752, %752* %1, i64 0, i32 0
  %31 = load i64, i64* %30, align 8, !range !220
  %32 = getelementptr inbounds %752, %752* %1, i64 0, i32 1
  %33 = icmp eq i64 %31, 0
  br i1 %33, label %38, label %34

34:                                               ; preds = %29
  %35 = bitcast %529** %3 to i8*
  call void @llvm.lifetime.start.p0i8(i64 8, i8* nonnull %35), !noalias !99602
  %36 = bitcast %529** %3 to [10 x i64]**
  store [10 x i64]* %32, [10 x i64]** %36, align 8, !noalias !99602
  %37 = bitcast %529** %3 to {}*
  call void @_ZN4core6result13unwrap_failed17h0f27636d1d025391E([0 x i8]* noalias nonnull readonly align 1 bitcast (<{ [43 x i8] }>* @13883 to [0 x i8]*), i64 43, {}* nonnull align 1 %37, [3 x i64]* noalias readonly align 8 dereferenceable(24) bitcast (<{ i8*, [16 x i8], i8*, [0 x i8] }>* @6297 to [3 x i64]*), %71* noalias nonnull readonly align 8 dereferenceable(24) bitcast (<{ i8*, [16 x i8] }>* @6302 to %71*)) #104
  unreachable

38:                                               ; preds = %29
  %39 = bitcast [10 x i64]* %32 to %135*
  %40 = bitcast [10 x i64]* %32 to i8*
  %41 = load i8, i8* %40, align 8, !range !1540
  %42 = icmp eq i8 %41, 3
  %43 = getelementptr inbounds %135, %135* %39, i64 0, i32 1, i64 0
  %44 = load i8, i8* %43, align 1
  %45 = select i1 %42, i8 %44, i8 2
  br label %NodeBlock

NodeBlock:                                        ; preds = %38
  %Pivot = icmp slt i8 %45, 2
  br i1 %Pivot, label %LeafBlock, label %LeafBlock21

LeafBlock21:                                      ; preds = %NodeBlock
  %SwitchLeaf22 = icmp eq i8 %45, 2
  br i1 %SwitchLeaf22, label %46, label %NewDefault

LeafBlock:                                        ; preds = %NodeBlock
  %SwitchLeaf = icmp eq i8 %45, 0
  br i1 %SwitchLeaf, label %47, label %NewDefault

46:                                               ; preds = %LeafBlock21
  call void @_ZN4core9panicking5panic17h367b69984712bd50E([0 x i8]* noalias nonnull readonly align 1 bitcast (<{ [43 x i8] }>* @13881 to [0 x i8]*), i64 43, %71* noalias nonnull readonly align 8 dereferenceable(24) bitcast (<{ i8*, [16 x i8] }>* @6303 to %71*)) #104
  unreachable

47:                                               ; preds = %LeafBlock, %71
  ret void

NewDefault:                                       ; preds = %LeafBlock21, %LeafBlock
  br label %48

48:                                               ; preds = %NewDefault
  call void @llvm.experimental.noalias.scope.decl(metadata !99605)
  call void @llvm.lifetime.start.p0i8(i64 40, i8* nonnull %13), !noalias !99605
  call fastcc void @17203(%135* noalias nocapture nonnull dereferenceable(40) %14, %135* nonnull align 8 dereferenceable(40) bitcast ([40 x i8]* @16148 to %135*)) #104, !noalias !99605
  invoke fastcc void @17183(%135* nonnull %39)
          to label %51 unwind label %49

49:                                               ; preds = %48
  %50 = landingpad { i8*, i32 }
          cleanup
  store i64 0, i64* %30, align 8, !alias.scope !99605
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* noundef nonnull align 8 dereferenceable(40) %40, i8* noundef nonnull align 8 dereferenceable(40) %13, i64 40, i1 false)
  br label %common.resume

51:                                               ; preds = %48
  store i64 0, i64* %30, align 8, !alias.scope !99605
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* noundef nonnull align 8 dereferenceable(40) %40, i8* noundef nonnull align 8 dereferenceable(40) %13, i64 40, i1 false)
  call void @llvm.lifetime.end.p0i8(i64 40, i8* nonnull %13), !noalias !99605
  call void @llvm.experimental.noalias.scope.decl(metadata !99608)
  %52 = getelementptr inbounds %135, %135* %8, i64 0, i32 0
  call void @llvm.lifetime.start.p0i8(i64 40, i8* nonnull %52), !noalias !99611
  call fastcc void @17203(%135* noalias nocapture nonnull dereferenceable(40) %8, %135* nonnull align 8 dereferenceable(40) %39) #104, !noalias !99611
  %53 = bitcast %116* %7 to i8*
  call void @llvm.lifetime.start.p0i8(i64 24, i8* nonnull %53), !noalias !99611
  %54 = getelementptr inbounds %142, %142* %0, i64 0, i32 0, i32 0
  %55 = load {}*, {}** %54, align 8, !alias.scope !99613, !noalias !99616, !nonnull !1
  %56 = getelementptr inbounds %142, %142* %0, i64 0, i32 0, i32 1
  %57 = load [3 x i64]*, [3 x i64]** %56, align 8, !alias.scope !99613, !noalias !99616, !nonnull !1
  %58 = getelementptr inbounds %135, %135* %6, i64 0, i32 0
  call void @llvm.lifetime.start.p0i8(i64 40, i8* nonnull %58), !noalias !99611
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* noundef nonnull align 8 dereferenceable(40) %58, i8* noundef nonnull align 8 dereferenceable(40) %52, i64 40, i1 false), !noalias !99611
  %59 = getelementptr inbounds [3 x i64], [3 x i64]* %57, i64 0, i64 4
  %60 = bitcast i64* %59 to void (%116*, {}*, %74*, %135*)**
  %61 = load void (%116*, {}*, %74*, %135*)*, void (%116*, {}*, %74*, %135*)** %60, align 8, !invariant.load !1, !noalias !99611, !nonnull !1
  call void %61(%116* noalias nocapture nonnull sret(%116) dereferenceable(24) %7, {}* nonnull align 1 %55, %74* noalias nonnull readonly align 8 dereferenceable(32) bitcast ([32 x i8]* @16149 to %74*), %135* noalias nocapture nonnull dereferenceable(40) %6) #104, !noalias !99608
  call void @llvm.lifetime.end.p0i8(i64 40, i8* nonnull %58), !noalias !99611
  %62 = getelementptr inbounds %116, %116* %7, i64 0, i32 0
  %63 = load {}*, {}** %62, align 8, !noalias !99611
  %64 = icmp eq {}* %63, null
  %65 = bitcast {}* %63 to i8*
  br i1 %64, label %71, label %66

66:                                               ; preds = %51
  %67 = getelementptr inbounds %116, %116* %7, i64 0, i32 1, i64 0
  %68 = load i64, i64* %67, align 8, !noalias !99611
  %69 = icmp eq i64 %68, 0
  br i1 %69, label %71, label %70

70:                                               ; preds = %66
  call void @__rust_dealloc(i8* nonnull %65, i64 %68, i64 1) #104, !noalias !99608
  br label %71

71:                                               ; preds = %51, %66, %70
  call void @llvm.lifetime.end.p0i8(i64 24, i8* nonnull %53), !noalias !99611
  call void @llvm.lifetime.end.p0i8(i64 40, i8* nonnull %52), !noalias !99611
  br label %47
}
```

Note the batched stack allocations, inlining of function calls and consolidation
of control flow.

The behavior that occurs when `panic`ing inside the Rust stubs can be controlled
by linking either the `panic_unwind*.bc` or `panic_abort*.bc` files, analogous
to
[setting the `panic` key in `Cargo.toml`](https://doc.rust-lang.org/book/ch09-01-unrecoverable-errors-with-panic.html#unwinding-the-stack-or-aborting-in-response-to-a-panic).
We use the same strategy that is used in our Vector binary, which currently uses
the default "unwind".

### Testing Strategy

Unit tests: Add tests for the code generation of each expression in isolation,
making sure that the emitted LLVM IR passes static analysis and the generated
code produces the expected result. This covers edge cases specific to each
expression.

Behavior tests: Make sure that the existing test corpus living in
[`lib/vrl/tests/tests`](https://github.com/vectordotdev/vector/blob/master/lib/vrl/tests/tests)
and
[`tests/behavior/transforms/remap.toml`](https://github.com/vectordotdev/vector/blob/master/tests/behavior/transforms/remap.toml)
passes when using the LLVM based execution engine.

Benchmark tests: Run micro-benchmarks that compare the runtime of VRL scripts in
varying complexity for each execution mode. The LLVM based approach should
conceptually always be the fastest. Should we discover a case where that doesn't
hold, we can examine it closely to see where the other execution engines apply
optimizations that we left out.

Soak tests: Run end-to-end tests to observe the impact on overall performance in
a pipeline. Again, the LLVM based approach should be fastest in every case. The
overall speedup will also largely depend on how heavy the remap script is and if
there exist other components that bottleneck the pipeline.

Fuzz tests: Run VRL programs that integrate a combination of arbitrary
expressions that are automatically generated. This can uncover faults in very
fringe edge cases that only occur when specific expressions interact with each
other. We can use the existing execution modes to cross-validate for
correctness.

Manual review: While static analysis tools and automated tests prevent a certain
class of bugs, there's still many logic errors that can occur in `unsafe` code
which can lead to memory corruption when invariants are violated. For one
measure, we can save LLVM IR in textual form in
[`lib/vrl/tests/tests/expressions`](https://github.com/vectordotdev/vector/blob/master/lib/vrl/tests/tests/expressions)
such that manual verification of generated code can be incorporated into the
pull review process. We might also add review guidelines that e.g. require
adding a label `unsafe` to the pull request (ideally this can be automated), and
requests reviewers to explicitly acknowledge that they have reviewed the unsafe
blocks in question.

## Rationale

As long as the single-core performance of VRL is the bottleneck of a topology,
general performance improvements to VRL are extremely valuable as they equate to
an equally sized performance improvement to the entire topology.

We want to live up to the
[performance guarantees](https://github.com/vectordotdev/vector/blob/f1404bea186ba83c4426a32bbef3f633c17cf4d2/website/cue/reference/remap/features/compilation.cue#L4-L8)
outlined in VRL's list of features.

Every VRL program benefits from the reduced runtime overhead, without us needing
to optimize any specific use case. Consistent execution speed is important to
build trust in the language.

Being able to execute our log transformation DSL at speeds which would otherwise
only be attainable by hand-writing Rust programs will strengthen a key value
proposition of Vector: best-in-class performance.

## Drawbacks

By generating machine code via LLVM, we are no longer (largely) immune to memory
violations. To reduce the error surface as much as possible, we employ industry
practices such as fuzz-testing VRL programs, static analysis via LLVM and rely
on the Rust compiler for any non-trivial code fragments. That being said, memory
safety guarantees always rely on a small link of trust that can not be
automatically verified - this time we are ourselves responsible for maintaining
a small set of invariants instead of being able to defer to a third party for
correctness. We still provide an inherently memory safe language to the user.

Producing LLVM bitcode for Rust's `std` library is guarded behind the
[`-Z build-std`](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std)
flag and only available on the nightly compiler toolchain. We need `std` to
fully link our precompiled LLVM bitcode. It's possible to circumvent the nightly
requirement by setting the `RUSTC_BOOTSTRAP=1` environment variable, such that
we have a `std` that is built using the same Rust and LLVM version as Vector. We
isolate the usage of this hack by building a separate crate with `std` only, and
link it to the library bitcode in a build step.

Statically linking LLVM to the Vector binary adds roughly 9MB, additionally to
precompiled bitcode that needs to be included with the binary. If this is a
concern, we can consider shipping binaries with the LLVM feature disabled.

While LLVM is a highly used framework within the industry, working with it
requires rather specialized knowledge about compiler construction. However,
there exists plenty of publicly accessible material for code generation using
LLVM, some of which I linked to [above](#introduction-to-llvm). In addition,
there should be great focus to document and test this part of the code base
extraordinarily well.

## Alternatives

### Compile to Rust

Using Rust as a compilation target would require us to

- ship a Rust compiler and its libraries
- ship Vector source code and its dependent crates

which would be hundreds of MB and therefore infeasible.

### Compile to C

Using C as a compilation target would require us to

- ship a C compiler

while

- not having any better safety guarantees
- not being able to inline functions and therefore miss optimization potential

and therefore not provide any significant benefits over using LLVM directly.

### Compile to WebAssembly

Using WebAssembly as a compilation target would require us to

- ship a Wasm runtime
- copy data in and out of WebAssembly or use `mmap`ing techniques which would
  constrain in which memory regions event data must reside

On the upside, WebAssembly provides a higher abstraction level and semantics
that allow to execute untrusted code to safely, however at the cost of slower
execution speed.

Tangentially related to this consideration stands the fact we recently dropped
support for the WebAssembly transform.

### Compile to Bitcode

As mentioned in the context, we are currently moving forward with a VM for VRL.
Compared to the current execution model and an LLVM-based approach, the VM
provides a middle ground for execution speed, memory safety and sophistication.

Weighing the benefits depends on the real world performance of both approaches.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues
after the RFC is approved:

- Submit a PR with spike-level code _roughly_ demonstrating the change:
  [#10442](https://github.com/vectordotdev/vector/pull/10442).
- Extract a core library from VRL for exposing its types with minimal
  dependencies, necessary to reduce size of the precompiled bitcode.
- Get feature parity close enough to run first soak tests against current
  execution model to get a first peek on end-to-end performance.
- Define conventions around optional, named and compiled function arguments.
- Refine code generation by taking into account type information.
- Add unit tests for each expression in isolation.
- Add fuzz tests that cross-validate results of all three execution modes.
- Investigate if heap allocations use the same strategy as our main Vector
  binary and are covered by our regular performance analysis tools

---

[^1]:
    It certainly doesn't help that "LLVM was originally an initialism for Low
    Level Virtual Machine". However, the "LLVM abbreviation has officially been
    removed to avoid confusion, as LLVM has evolved into an umbrella project
    that has little relationship to what most current developers think of as
    (more specifically) process virtual machines."
    [↪](https://en.wikipedia.org/wiki/LLVM#History)
