use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription},
    event::ValueKind,
    Event,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum CheckFieldsPredicateArg {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

pub trait CheckFieldsPredicate: std::fmt::Debug + Send + Sync {
    fn check(&self, e: &Event) -> bool;
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct EqualsPredicate {
    target: Atom,
    arg: CheckFieldsPredicateArg,
}

impl EqualsPredicate {
    pub fn new(
        target: String,
        arg: &CheckFieldsPredicateArg,
    ) -> Result<Box<dyn CheckFieldsPredicate>, String> {
        Ok(Box::new(Self {
            target: target.into(),
            arg: arg.clone(),
        }))
    }
}

impl CheckFieldsPredicate for EqualsPredicate {
    fn check(&self, event: &Event) -> bool {
        match event {
            Event::Log(l) => l.get(&self.target).map_or(false, |v| match &self.arg {
                CheckFieldsPredicateArg::String(s) => s.as_bytes() == v.as_bytes(),
                CheckFieldsPredicateArg::Integer(i) => match v {
                    ValueKind::Integer(vi) => *i == *vi,
                    ValueKind::Float(vf) => *i == *vf as i64,
                    _ => false,
                },
                CheckFieldsPredicateArg::Float(f) => match v {
                    ValueKind::Float(vf) => *f == *vf,
                    ValueKind::Integer(vi) => *f == *vi as f64,
                    _ => false,
                },
                CheckFieldsPredicateArg::Boolean(b) => match v {
                    ValueKind::Boolean(vb) => *b == *vb,
                    _ => false,
                },
            }),
            Event::Metric(m) => m
                .tags()
                .as_ref()
                .and_then(|t| t.get(self.target.as_ref()))
                .map_or(false, |v| match &self.arg {
                    CheckFieldsPredicateArg::String(s) => s.as_bytes() == v.as_bytes(),
                    _ => false,
                }),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct NotEqualsPredicate {
    target: Atom,
    arg: String,
}

impl NotEqualsPredicate {
    pub fn new(
        target: String,
        arg: &CheckFieldsPredicateArg,
    ) -> Result<Box<dyn CheckFieldsPredicate>, String> {
        Ok(Box::new(Self {
            target: target.into(),
            arg: match arg {
                CheckFieldsPredicateArg::String(s) => s.clone(),
                CheckFieldsPredicateArg::Integer(a) => format!("{}", a),
                CheckFieldsPredicateArg::Float(a) => format!("{}", a),
                CheckFieldsPredicateArg::Boolean(a) => format!("{}", a),
            },
        }))
    }
}

impl CheckFieldsPredicate for NotEqualsPredicate {
    fn check(&self, event: &Event) -> bool {
        match event {
            Event::Log(l) => l
                .get(&self.target)
                .map(|f| f.as_bytes())
                .map_or(false, |b| b != self.arg.as_bytes()),
            Event::Metric(m) => m
                .tags()
                .as_ref()
                .and_then(|t| t.get(self.target.as_ref()))
                .map_or(false, |v| v.as_bytes() != self.arg.as_bytes()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ExistsPredicate {
    target: Atom,
    arg: bool,
}

impl ExistsPredicate {
    pub fn new(
        target: String,
        arg: &CheckFieldsPredicateArg,
    ) -> Result<Box<dyn CheckFieldsPredicate>, String> {
        match arg {
            CheckFieldsPredicateArg::Boolean(b) => Ok(Box::new(Self {
                target: target.into(),
                arg: *b,
            })),
            _ => Err("exists predicate requires a boolean argument".to_owned()),
        }
    }
}

impl CheckFieldsPredicate for ExistsPredicate {
    fn check(&self, event: &Event) -> bool {
        (match event {
            Event::Log(l) => l.get(&self.target).is_some(),
            Event::Metric(m) => m
                .tags()
                .as_ref()
                .map_or(false, |t| t.contains_key(self.target.as_ref())),
        }) == self.arg
    }
}

//------------------------------------------------------------------------------

fn build_predicate(
    predicate: &str,
    target: String,
    arg: &CheckFieldsPredicateArg,
) -> Result<Box<dyn CheckFieldsPredicate>, String> {
    match predicate {
        "eq" | "equals" => EqualsPredicate::new(target, arg),
        "neq" | "not_equals" => NotEqualsPredicate::new(target, arg),
        "exists" => ExistsPredicate::new(target, arg),
        _ => Err(format!("predicate type '{}' not recognized", predicate)),
    }
}

fn build_predicates(
    map: &IndexMap<String, CheckFieldsPredicateArg>,
) -> Result<Vec<Box<dyn CheckFieldsPredicate>>, Vec<String>> {
    let mut predicates: Vec<Box<dyn CheckFieldsPredicate>> = Vec::new();
    let mut errors = Vec::new();

    for (target_pred, arg) in map {
        if target_pred
            .rfind('.')
            .and_then(|i| {
                if i > 0 && i < target_pred.len() - 1 {
                    Some(i)
                } else {
                    None
                }
            })
            .and_then(|i| {
                let mut target = target_pred.clone();
                let pred = target.split_off(i + 1);
                target.truncate(target.len() - 1);
                match build_predicate(&pred, target, arg) {
                    Ok(pred) => predicates.push(pred),
                    Err(err) => errors.push(err),
                }
                Some(())
            })
            .is_none()
        {
            errors.push(format!("predicate not found in check_fields value '{}', format must be <target>.<predicate>", target_pred));
        }
    }

    if errors.is_empty() {
        Ok(predicates)
    } else {
        Err(errors)
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct CheckFieldsConfig {
    #[serde(flatten, default)]
    predicates: IndexMap<String, CheckFieldsPredicateArg>,
}

inventory::submit! {
    ConditionDescription::new::<CheckFieldsConfig>("check_fields")
}

#[typetag::serde(name = "check_fields")]
impl ConditionConfig for CheckFieldsConfig {
    fn build(&self) -> crate::Result<Box<dyn Condition>> {
        build_predicates(&self.predicates)
            .map(|preds| -> Box<dyn Condition> { Box::new(CheckFields { predicates: preds }) })
            .map_err(|errs| {
                if errs.len() > 1 {
                    let mut err_fmt = errs.join("\n");
                    err_fmt.insert_str(0, "failed to parse predicates:\n");
                    err_fmt
                } else {
                    errs[0].clone()
                }
                .into()
            })
    }
}

//------------------------------------------------------------------------------

pub struct CheckFields {
    predicates: Vec<Box<dyn CheckFieldsPredicate>>,
}

impl Condition for CheckFields {
    fn check(&self, e: &Event) -> bool {
        self.predicates.iter().find(|p| !p.check(e)).is_none()
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate::Event;

    #[test]
    fn check_predicate_errors() {
        let cases = vec![
            ("foo", "predicate not found in check_fields value 'foo', format must be <target>.<predicate>"),
            (".nah", "predicate not found in check_fields value '.nah', format must be <target>.<predicate>"),
            ("", "predicate not found in check_fields value '', format must be <target>.<predicate>"),
            ("what.", "predicate not found in check_fields value 'what.', format must be <target>.<predicate>"),
            ("foo.not_real", "predicate type 'not_real' not recognized"),
        ];

        let mut aggregated_preds: IndexMap<String, CheckFieldsPredicateArg> = IndexMap::new();
        let mut exp_errs = Vec::new();
        for (pred, exp) in cases {
            aggregated_preds.insert(pred.into(), CheckFieldsPredicateArg::String("foo".into()));
            exp_errs.push(exp);

            let mut preds: IndexMap<String, CheckFieldsPredicateArg> = IndexMap::new();
            preds.insert(pred.into(), CheckFieldsPredicateArg::String("foo".into()));

            assert_eq!(
                CheckFieldsConfig { predicates: preds }
                    .build()
                    .err()
                    .unwrap()
                    .to_string(),
                exp.to_owned()
            );
        }

        let mut exp_err = exp_errs.join("\n");
        exp_err.insert_str(0, "failed to parse predicates:\n");

        assert_eq!(
            CheckFieldsConfig {
                predicates: aggregated_preds
            }
            .build()
            .err()
            .unwrap()
            .to_string(),
            exp_err
        );
    }

    #[test]
    fn check_field_equals() {
        let mut preds: IndexMap<String, CheckFieldsPredicateArg> = IndexMap::new();
        preds.insert(
            "message.equals".into(),
            CheckFieldsPredicateArg::String("foo".into()),
        );
        preds.insert(
            "other_thing.eq".into(),
            CheckFieldsPredicateArg::String("bar".into()),
        );

        let cond = CheckFieldsConfig { predicates: preds }.build().unwrap();

        let mut event = Event::from("foo");
        assert_eq!(cond.check(&event), false);

        event
            .as_mut_log()
            .insert_implicit("other_thing".into(), "bar".into());
        assert_eq!(cond.check(&event), true);

        event
            .as_mut_log()
            .insert_implicit("message".into(), "not foo".into());
        assert_eq!(cond.check(&event), false);
    }

    #[test]
    fn check_field_not_equals() {
        let mut preds: IndexMap<String, CheckFieldsPredicateArg> = IndexMap::new();
        preds.insert(
            "message.not_equals".into(),
            CheckFieldsPredicateArg::String("foo".into()),
        );
        preds.insert(
            "other_thing.neq".into(),
            CheckFieldsPredicateArg::String("bar".into()),
        );

        let cond = CheckFieldsConfig { predicates: preds }.build().unwrap();

        let mut event = Event::from("not foo");
        assert_eq!(cond.check(&event), false);

        event
            .as_mut_log()
            .insert_implicit("other_thing".into(), "not bar".into());
        assert_eq!(cond.check(&event), true);

        event
            .as_mut_log()
            .insert_implicit("other_thing".into(), "bar".into());
        assert_eq!(cond.check(&event), false);

        event
            .as_mut_log()
            .insert_implicit("message".into(), "foo".into());
        assert_eq!(cond.check(&event), false);
    }

    #[test]
    fn check_field_exists() {
        let mut preds: IndexMap<String, CheckFieldsPredicateArg> = IndexMap::new();
        preds.insert("foo.exists".into(), CheckFieldsPredicateArg::Boolean(true));
        preds.insert("bar.exists".into(), CheckFieldsPredicateArg::Boolean(false));

        let cond = CheckFieldsConfig { predicates: preds }.build().unwrap();

        let mut event = Event::from("ignored field");
        assert_eq!(cond.check(&event), false);

        event
            .as_mut_log()
            .insert_implicit("foo".into(), "not ignored".into());
        assert_eq!(cond.check(&event), true);

        event
            .as_mut_log()
            .insert_implicit("bar".into(), "also not ignored".into());
        assert_eq!(cond.check(&event), false);
    }
}
