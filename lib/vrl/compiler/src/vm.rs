//! VRL Virtual Machine
//!
//! This implements a virtual machine for running VRL code.
//! The machine instructions are stored in a `Vec<Instruction>`.
//!
//! `Instruction` is an enum that can either be an `OpCode` or a
//! `Primitive`.
//!
//! Interpretation of the VM is essentially a process of looping through
//! the instructions. A large match over the `OpCode` interprets each
//! step accordingly.
//!
//! # OpCode
//! `OpCode`s is an enum of different instructions for the machine to
//! interpret.
//!
//! `Primitive` contains a `usize` that provides different parameters
//! for the current `OpCode` to use.
//!
//! The VM contains a number of fields of static data that the instructions
//! index into using the primitive parameters provided.
//!
//! # Values
//! `values` contains static literal `Value`s that have been created during
//! compilation.
//!
//! # Targets
//! `targets` contains any lookup paths that are used in the code. These
//! can be  `Internal`, `External` or `Stack` paths.
//! A `Stack` path is a path used to lookup a field in the value (either an
//! object or an array) found at the top of the stack - typically the return
//! value from a function or a static literal.
//!
//! # Static Params
//! `static_params` contains a `Vec` of `dyn std::any::Any`. These parameters
//! are created by functions in the `stdlib` that need to cache parameters
//! calculated during compilation. The index of the paramter is passed to the
//! function during runtime, allowing it to downcast the data to the correct
//! type and use as necessary.

mod argument_list;
mod machine;
mod state;
mod variable;

pub use argument_list::VmArgumentList;
pub use machine::OpCode;
pub use machine::Vm;
pub use variable::Variable;
