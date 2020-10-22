use crate::{Expression, Object, Program, Result, State, Value};

pub struct Runtime {
    state: State,

    /// Whether to allow mutation during execution.
    ///
    /// This flag block _persistent_ mutation, meaning any changes to `Object`
    /// are disallowed, but defining temporary variables is still possible.
    ///
    /// TODO: wire this up so it actually works.
    mutation: bool,
}

impl Runtime {
    pub fn new(state: State) -> Self {
        Self {
            state,
            mutation: true,
        }
    }

    /// Set whether the runtime is allowed to mutate any [`Object`] it executes
    /// on.
    ///
    /// If set to `false`, any attempt to mutate the object from the program
    /// results in an error.
    ///
    /// This flag is useful if you want to provide access to the language in a
    /// scope where you want the user to be able to query the object, and return
    /// a given value based on the object's properties, but _don't_ want the
    /// object to be mutated during execution.
    pub fn mutable(&mut self, mutation: bool) {
        self.mutation = mutation;
    }

    /// Given the provided [`Object`], run the provided [`Program`] to
    /// completion.
    pub fn execute(&mut self, mut object: impl Object, program: Program) -> Result<Option<Value>> {
        let mut values = program
            .expressions
            .iter()
            .map(|expression| expression.execute(&mut self.state, &mut object))
            .collect::<Result<Vec<Option<Value>>>>()?;

        Ok(values.pop().flatten())
    }
}
