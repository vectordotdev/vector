use super::Function;
use crate::{
    event::{util::log::get_value, Event, PathIter, Value},
    mapping::Result,
};
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub struct Path {
    // TODO: Switch to String once Event API is cleaned up.
    path: Vec<Vec<Atom>>,
}

impl From<&str> for Path {
    fn from(target: &str) -> Self {
        Self {
            path: vec![vec![target.into()]],
        }
    }
}

impl From<Vec<Vec<String>>> for Path {
    fn from(path: Vec<Vec<String>>) -> Self {
        Self {
            // TODO: Switch to String once Event API is cleaned up.
            path: path
                .iter()
                .map(|c| c.iter().map(|p| Atom::from(p.clone())).collect())
                .collect(),
        }
    }
}

impl From<Vec<Vec<&str>>> for Path {
    fn from(path: Vec<Vec<&str>>) -> Self {
        Self {
            // TODO: Switch to String once Event API is cleaned up.
            path: path
                .iter()
                .map(|c| c.iter().map(|p| Atom::from(p.clone())).collect())
                .collect(),
        }
    }
}

impl Function for Path {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        // For some reason we can't walk a Value from the root of the log, and
        // so we need to pull out the first path segment manually.
        let mut value = self.path[0]
            .iter()
            .find_map(|p| ctx.as_log().get(p))
            .ok_or(format!(
                "path .{} not found in event",
                self.path[0].first().unwrap()
            ))?;

        // Walk remaining (if any) path segments.
        for (i, segments) in self.path.iter().enumerate().skip(1) {
            value = segments
                .iter()
                .find_map(|p| get_value(value, PathIter::new(p)))
                .ok_or(format!(
                    "path {} not found in event",
                    self.path
                        .iter()
                        .take(i + 1)
                        .fold("".to_string(), |acc, p| format!(
                            "{}.{}",
                            acc,
                            p.first().unwrap()
                        ),)
                ))?;
        }

        Ok(value.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn check_path_query() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                Path::from(vec![vec!["foo"]]),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event
                },
                Ok(Value::from(json!("bar"))),
                Path::from(vec![vec!["foo"]]),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event
                },
                Err("path .foo.bar not found in event".to_string()),
                Path::from(vec![vec!["foo"], vec!["bar"]]),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("bar"));
                    event
                },
                Ok(Value::from(json!("bar"))),
                Path::from(vec![vec!["bar", "foo"]]),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo.baz", Value::from("buz"));
                    event
                },
                Ok(Value::from(json!("buz"))),
                Path::from(vec![vec!["foo"], vec!["bar", "baz"]]),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
