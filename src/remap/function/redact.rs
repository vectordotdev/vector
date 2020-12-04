use remap::prelude::*;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub struct Redact;

impl Function for Redact {
    fn identifier(&self) -> &'static str {
        "redact"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "filters",
                accepts: |v| matches!(v, Value::Array(_)),
                required: false,
            },
            Parameter {
                keyword: "redactor",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "patterns",
                accepts: |v| matches!(v, Value::Array(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;

        let filters = arguments
            .optional_enum_list("filters", &Filter::all_str())?
            .unwrap_or_default()
            .into_iter()
            .map(|s| Filter::from_str(&s).expect("validated enum"))
            .collect::<Vec<_>>();

        let redactor = arguments
            .optional_enum("redactor", &Redactor::all_str())?
            .map(|s| Redactor::from_str(&s).expect("validated enum"))
            .unwrap_or_default();

        let patterns = arguments.optional_array("patterns")?;

        Ok(Box::new(RedactFn {
            value,
            filters,
            redactor,
            patterns,
        }))
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RedactFn {
    value: Box<dyn Expression>,
    filters: Vec<Filter>,
    redactor: Redactor,
    patterns: Option<Vec<Argument>>,
}

impl Expression for RedactFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let mut input = String::from_utf8_lossy(&bytes).into_owned();

        for filter in &self.filters {
            match filter {
                Filter::Pattern => self
                    .patterns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .try_for_each::<_, Result<()>>(|pattern| match pattern {
                        Argument::Regex(regex) => {
                            input = regex
                                .replace_all(&input, self.redactor.pattern())
                                .into_owned();
                            Ok(())
                        }
                        Argument::Expression(expr) => {
                            let bytes = expr.execute(state, object)?.try_bytes()?;
                            let pattern = String::from_utf8_lossy(&bytes);

                            input = input.replace(pattern.as_ref(), self.redactor.pattern());
                            Ok(())
                        }
                        _ => unimplemented!(),
                    })?,
            }
        }

        Ok(input.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .with_constraint(value::Kind::Bytes)
    }
}

// -----------------------------------------------------------------------------

/// The redaction filter to apply to the given value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Pattern,
}

impl Filter {
    fn all_str() -> Vec<&'static str> {
        use Filter::*;

        vec![Pattern]
            .into_iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Filter::*;

        match self {
            Pattern => "pattern",
        }
    }
}

impl FromStr for Filter {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Filter::*;

        match s {
            "pattern" => Ok(Pattern),
            _ => Err("unknown filter"),
        }
    }
}

// -----------------------------------------------------------------------------

/// The recipe for redacting the matched filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Redactor {
    Full,
}

impl Redactor {
    fn all_str() -> Vec<&'static str> {
        use Redactor::*;

        vec![Full]
            .into_iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
    }

    fn as_str(self) -> &'static str {
        use Redactor::*;

        match self {
            Full => "full",
        }
    }

    fn pattern(&self) -> &str {
        use Redactor::*;

        match self {
            Full => "****",
        }
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Redactor::Full
    }
}

impl FromStr for Redactor {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Redactor::*;

        match s {
            "full" => Ok(Full),
            _ => Err("unknown redactor"),
        }
    }
}
