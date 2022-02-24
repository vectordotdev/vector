use std::ops::{Deref, DerefMut};

use lookup::LookupBuf;
use vrl::prelude::*;

pub struct MeaningList(pub BTreeMap<String, LookupBuf>);

impl Deref for MeaningList {
    type Target = BTreeMap<String, LookupBuf>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MeaningList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SetSemanticMeaning;

impl Function for SetSemanticMeaning {
    fn identifier(&self) -> &'static str {
        "set_semantic_meaning"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "target",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "meaning",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Sets custom field semantic meaning",
            source: r#"set_semantic_meaning(.foo, "bar")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let query = arguments.required_query("target")?;

        let meaning = arguments
            .required_literal("meaning")?
            .to_value()
            .try_bytes_utf8_lossy()
            .expect("meaning not bytes")
            .into_owned();

        if !query.is_external() {
            return Err(Box::new(ExpressionError::from(format!(
                "meaning must be set on an external field: {}",
                query
            ))) as Box<dyn DiagnosticError>);
        }

        if let Some(list) = ctx.get_external_context_mut::<MeaningList>() {
            list.insert(meaning, query.path().clone());
        };

        Ok(Box::new(SetSemanticMeaningFn))
    }
}

#[derive(Debug, Clone)]
struct SetSemanticMeaningFn;

impl Expression for SetSemanticMeaningFn {
    fn resolve(&self, _ctx: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::null().infallible()
    }
}
