use crate::{
    conditions::Condition,
    event::Event,
    topology::config::{TestCondition, TestDefinition, TestInputValue},
    transforms::Transform,
};
use std::collections::HashMap;

//------------------------------------------------------------------------------

pub struct UnitTestCheck {
    extract_from: String,
    conditions: HashMap<String, Box<dyn Condition>>,
}

pub struct UnitTestTransform {
    transform: Box<dyn Transform>,
    next: Vec<String>,
}

pub struct UnitTest {
    pub name: String,
    input: (String, Event),
    transforms: HashMap<String, UnitTestTransform>,
    checks: Vec<UnitTestCheck>,
}

//------------------------------------------------------------------------------

fn event_to_string(event: Event) -> String {
    match event {
        Event::Log(log) => serde_json::to_string(&log.unflatten()).unwrap_or_else(|_| "{}".into()),
        Event::Metric(metric) => serde_json::to_string(&metric).unwrap_or_else(|_| "{}".into()),
    }
}

fn walk(
    node: &str,
    inputs: Vec<Event>,
    transforms: &mut HashMap<String, UnitTestTransform>,
    aggregated_results: &mut HashMap<String, Vec<Event>>,
) {
    let mut results = Vec::new();
    let mut targets = Vec::new();

    if let Some(target) = transforms.get_mut(node) {
        for input in inputs {
            target.transform.transform_into(&mut results, input);
        }
        targets = target.next.clone();
    }

    for child in targets {
        walk(&child, results.clone(), transforms, aggregated_results);
    }
    aggregated_results.insert(node.into(), results);
}

impl UnitTest {
    pub fn run(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut results = HashMap::new();

        walk(
            &self.input.0,
            vec![self.input.1.clone()],
            &mut self.transforms,
            &mut results,
        );

        for check in &self.checks {
            if let Some(results) = results.get(&check.extract_from) {
                let mut failed_conditions = Vec::new();
                for (name, cond) in &check.conditions {
                    if results.iter().find(|e| cond.check(e)).is_none() {
                        failed_conditions.push(name.to_owned());
                    }
                }
                if !failed_conditions.is_empty() {
                    let event_strings: Vec<String> =
                        results.iter().map(|e| event_to_string(e.clone())).collect();
                    errors.push(format!(
                        "check transform '{}' failed conditions: [ {} ], payloads (encoded in JSON format):\n  {}",
                        check.extract_from,
                        failed_conditions.join(", "),
                        event_strings.join("\n  "),
                    ));
                }
            } else {
                errors.push(format!(
                    "check transform '{}' failed: received zero resulting events.",
                    check.extract_from,
                ));
            }
        }

        errors
    }
}

//------------------------------------------------------------------------------

fn links_to_a_leaf(
    target: &str,
    leaves: &HashMap<String, ()>,
    link_checked: &mut HashMap<String, bool>,
    transform_outputs: &HashMap<String, HashMap<String, ()>>,
) -> bool {
    if let Some(check) = link_checked.get(target) {
        return *check;
    }
    let has_linked_children = if let Some(outputs) = transform_outputs.get(target) {
        outputs
            .iter()
            .filter(|(o, _)| links_to_a_leaf(o, leaves, link_checked, transform_outputs))
            .count()
            > 0
    } else {
        false
    };
    let linked = leaves.contains_key(target) || has_linked_children;
    link_checked.insert(target.to_owned(), linked);
    linked
}

/// Reduces a collection of transforms into a set that only contains those that
/// link between our root (test input) and a set of leaves (test outputs).
fn reduce_transforms(
    root: &str,
    leaves: &HashMap<String, ()>,
    transform_outputs: &mut HashMap<String, HashMap<String, ()>>,
) {
    let mut link_checked: HashMap<String, bool> = HashMap::new();

    if !links_to_a_leaf(root, leaves, &mut link_checked, transform_outputs) {
        transform_outputs.clear();
    }

    transform_outputs.retain(|name, children| {
        let linked = name == root || *link_checked.get(name).unwrap_or(&false);
        if linked {
            // Also remove all unlinked children.
            children.retain(|child_name, _| {
                name == root || *link_checked.get(child_name).unwrap_or(&false)
            })
        }
        linked
    });
}

fn build_unit_test(
    definition: &TestDefinition,
    config: &super::Config,
) -> Result<UnitTest, Vec<String>> {
    let mut errors = vec![];

    // Build input event.
    let input_event = match definition.input.type_str.as_ref() {
        "raw" => match definition.input.value.as_ref() {
            Some(v) => Event::from(v.clone()),
            None => {
                errors.push(format!("input type 'raw' requires the field 'value'"));
                Event::from("")
            }
        },
        "log" => {
            if let Some(log_fields) = &definition.input.log_fields {
                let mut event = Event::from("");
                for (path, value) in log_fields {
                    event.as_mut_log().insert_explicit(
                        path.to_owned().into(),
                        match value {
                            TestInputValue::String(s) => s.as_bytes().into(),
                            TestInputValue::Boolean(b) => (*b).into(),
                            TestInputValue::Integer(i) => (*i).into(),
                            TestInputValue::Float(f) => (*f).into(),
                        },
                    );
                }
                event
            } else {
                errors.push(format!("input type 'log' requires the field 'log_fields'"));
                Event::from("")
            }
        }
        "metric" => {
            if let Some(metric) = &definition.input.metric {
                Event::Metric(metric.clone())
            } else {
                errors.push(format!("input type 'log' requires the field 'log_fields'"));
                Event::from("")
            }
        }
        _ => {
            errors.push(format!(
                "unrecognized input type '{}', expected one of: 'raw', 'log' or 'metric'",
                definition.input.type_str
            ));
            Event::from("")
        }
    };

    // Maps transform names with their output targets (transforms that use it as
    // an input).
    let mut transform_outputs: HashMap<String, HashMap<String, ()>> = config
        .transforms
        .iter()
        .map(|(k, _)| (k.clone(), HashMap::new()))
        .collect();

    config.transforms.iter().for_each(|(k, t)| {
        t.inputs.iter().for_each(|i| {
            if let Some(outputs) = transform_outputs.get_mut(i) {
                outputs.insert(k.to_string(), ());
            }
        })
    });

    if !transform_outputs.contains_key(&definition.input.insert_at) {
        errors.push(format!(
            "unable to locate target transform '{}'",
            definition.input.insert_at,
        ));
        return Err(errors);
    }

    let mut leaves: HashMap<String, ()> = HashMap::new();
    definition.outputs.iter().for_each(|o| {
        leaves.insert(o.extract_from.clone(), ());
    });

    // Reduce the configured transforms into just the ones connecting our test
    // target with output targets.
    reduce_transforms(&definition.input.insert_at, &leaves, &mut transform_outputs);

    // Build reduced transforms.
    let mut transforms: HashMap<String, UnitTestTransform> = HashMap::new();
    for (name, transform_config) in &config.transforms {
        if let Some(outputs) = transform_outputs.remove(name) {
            match transform_config.inner.build() {
                Ok(transform) => {
                    transforms.insert(
                        name.clone(),
                        UnitTestTransform {
                            transform: transform,
                            next: outputs.into_iter().map(|(k, _)| k).collect(),
                        },
                    );
                }
                Err(err) => {
                    errors.push(format!("failed to build transform '{}': {}", name, err));
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    definition.outputs.iter().for_each(|o| {
        if !transforms.contains_key(&o.extract_from) {
            errors.push(format!(
                "unable to complete topology between target transform '{}' and output target '{}'",
                definition.input.insert_at, o.extract_from
            ));
        }
    });

    // Build all output conditions.
    let checks = definition.outputs.iter().map(|o| {
        let mut conditions: HashMap<String, Box<dyn Condition>> = HashMap::new();
        for (k, cond_conf) in &o.conditions {
            match cond_conf {
                TestCondition::Embedded(b) => {
                    match b.build() {
                        Ok(c) => {
                            conditions.insert(k.clone(), c);
                        },
                        Err(e) => {
                            errors.push(format!(
                                "failed to create test condition '{}': {}",
                                k, e,
                            ));
                        },
                    }
                },
                TestCondition::String(_s) => {
                    errors.push(format!("failed to create test condition '{}': condition references are not yet supported", k));
                },
            }
        }
        UnitTestCheck{
            extract_from: o.extract_from.clone(),
            conditions: conditions,
        }
    }).collect();

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(UnitTest {
            name: definition.name.clone(),
            input: (definition.input.insert_at.clone(), input_event),
            transforms: transforms,
            checks: checks,
        })
    }
}

pub fn build_unit_tests(config: &super::Config) -> Result<Vec<UnitTest>, Vec<String>> {
    let mut tests = vec![];
    let mut errors = vec![];

    config
        .tests
        .iter()
        .for_each(|test| match build_unit_test(test, config) {
            Ok(t) => tests.push(t),
            Err(errs) => {
                let mut test_err = errs.join("\n");
                // Indent all line breaks
                test_err = test_err.replace("\n", "\n  ");
                test_err.insert_str(0, &format!("Failed to build test '{}':\n  ", test.name));
                errors.push(test_err);
            }
        });

    if errors.is_empty() {
        Ok(tests)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::config::Config;

    #[test]
    fn parse_no_input() {
        let config: Config = toml::from_str(
            r#"
[transforms.bar]
  inputs = ["foo"]
  type = "add_fields"
  [transforms.bar.fields]
    my_string_field = "string value"

[[tests]]
  name = "broken test"

  [tests.input]
    insert_at = "foo"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "bar"
    [tests.outputs.conditions.always_false]
      type = "check_fields"
      "#,
        )
        .unwrap();

        let errs = build_unit_tests(&config).err().unwrap();
        assert_eq!(
            errs,
            vec![r#"Failed to build test 'broken test':
  unable to locate target transform 'foo'"#
                .to_owned(),]
        );
    }

    #[test]
    fn parse_broken_topology() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["something"]
  type = "add_fields"
  [transforms.foo.fields]
    foo_field = "string value"

[transforms.baz]
  inputs = ["bar"]
  type = "add_fields"
  [transforms.baz.fields]
    baz_field = "string value"

[transforms.quz]
  inputs = ["bar"]
  type = "add_fields"
  [transforms.quz.fields]
    quz_field = "string value"

[[tests]]
  name = "broken test"

  [tests.input]
    insert_at = "foo"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [tests.outputs.conditions.always_false]
      type = "check_fields"
      "message.equals" = "not this"

  [[tests.outputs]]
    extract_from = "quz"
    [tests.outputs.conditions.always_false]
      type = "check_fields"
      "message.equals" = "not this"

[[tests]]
  name = "broken test 2"

  [tests.input]
    insert_at = "nope"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "quz"
    [tests.outputs.conditions.always_false]
      type = "check_fields"
      "message.equals" = "not this"
      "#,
        )
        .unwrap();

        let errs = build_unit_tests(&config).err().unwrap();
        assert_eq!(
            errs,
            vec![
                r#"Failed to build test 'broken test':
  unable to complete topology between target transform 'foo' and output target 'baz'
  unable to complete topology between target transform 'foo' and output target 'quz'"#
                    .to_owned(),
                r#"Failed to build test 'broken test 2':
  unable to locate target transform 'nope'"#
                    .to_owned(),
            ]
        );
    }

    #[test]
    fn parse_bad_input_event() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["ignored"]
  type = "add_fields"
  [transforms.foo.fields]
    my_string_field = "string value"

[[tests]]
  name = "broken test"

  [tests.input]
    insert_at = "foo"
    type = "nah"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "foo"
    [tests.outputs.conditions.always_false]
      type = "check_fields"
      "#,
        )
        .unwrap();

        let errs = build_unit_tests(&config).err().unwrap();
        assert_eq!(
            errs,
            vec![r#"Failed to build test 'broken test':
  unrecognized input type 'nah', expected one of: 'raw', 'log' or 'metric'"#
                .to_owned(),]
        );
    }

    #[test]
    fn test_success() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["ignored"]
  type = "add_fields"
  [transforms.foo.fields]
    new_field = "string value"

[transforms.bar]
  inputs = ["foo"]
  type = "add_fields"
  [transforms.bar.fields]
    second_new_field = "also a string value"

[transforms.baz]
  inputs = ["bar"]
  type = "add_fields"
  [transforms.baz.fields]
    third_new_field = "also also a string value"

[[tests]]
  name = "successful test"

  [tests.input]
    insert_at = "foo"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "foo"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "bar"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "third_new_field.equals" = "also also a string value"
      "message.equals" = "nah this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run(), Vec::<String>::new());
    }

    #[test]
    fn test_log_input() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["ignored"]
  type = "add_fields"
  [transforms.foo.fields]
    new_field = "string value"

[[tests]]
  name = "successful test with log event"

  [tests.input]
    insert_at = "foo"
    type = "log"
    [tests.input.log_fields]
      message = "this is the message"
      int_val = 5
      bool_val = true

  [[tests.outputs]]
    extract_from = "foo"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "message.equals" = "this is the message"
      "bool_val.eq" = true
      "int_val.eq" = 5
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run(), Vec::<String>::new());
    }

    #[test]
    fn test_metric_input() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["ignored"]
  type = "add_tags"
  [transforms.foo.tags]
    new_tag = "new value added"

[[tests]]
  name = "successful test with metric event"

  [tests.input]
    insert_at = "foo"
    type = "metric"
    [tests.input.metric]
      type = "counter"
      name = "foometric"
      val = 7
      [tests.input.metric.tags]
        tagfoo = "valfoo"

  [[tests.outputs]]
    extract_from = "foo"
    [tests.outputs.conditions.check_new_tag]
      type = "check_fields"
      "tagfoo.equals" = "valfoo"
      "new_tag.eq" = "new value added"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run(), Vec::<String>::new());
    }

    #[test]
    fn test_success_over_gap() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["ignored"]
  type = "add_fields"
  [transforms.foo.fields]
    new_field = "string value"

[transforms.bar]
  inputs = ["foo"]
  type = "add_fields"
  [transforms.bar.fields]
    second_new_field = "also a string value"

[transforms.baz]
  inputs = ["bar"]
  type = "add_fields"
  [transforms.baz.fields]
    third_new_field = "also also a string value"

[[tests]]
  name = "successful test"

  [tests.input]
    insert_at = "foo"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "third_new_field.equals" = "also also a string value"
      "message.equals" = "nah this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run(), Vec::<String>::new());
    }

    #[test]
    fn test_success_tree() {
        let config: Config = toml::from_str(
            r#"
[transforms.ignored]
  inputs = ["also_ignored"]
  type = "add_fields"
  [transforms.ignored.fields]
    not_field = "string value"

[transforms.foo]
  inputs = ["ignored"]
  type = "add_fields"
  [transforms.foo.fields]
    new_field = "string value"

[transforms.bar]
  inputs = ["foo"]
  type = "add_fields"
  [transforms.bar.fields]
    second_new_field = "also a string value"

[transforms.baz]
  inputs = ["foo"]
  type = "add_fields"
  [transforms.baz.fields]
    second_new_field = "also also a string value"

[[tests]]
  name = "successful test"

  [tests.input]
    insert_at = "foo"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "bar"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also also a string value"
      "message.equals" = "nah this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run(), Vec::<String>::new());
    }

    #[test]
    fn test_fails() {
        let config: Config = toml::from_str(
            r#"
[transforms.foo]
  inputs = ["ignored"]
  type = "remove_fields"
  fields = ["timestamp"]

[transforms.bar]
  inputs = ["foo"]
  type = "add_fields"
  [transforms.bar.fields]
    second_new_field = "also a string value"

[transforms.baz]
  inputs = ["bar"]
  type = "add_fields"
  [transforms.baz.fields]
    third_new_field = "also also a string value"

[[tests]]
  name = "failing test"

  [tests.input]
    insert_at = "foo"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "foo"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "bar"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "message.equals" = "not this"
    [tests.outputs.conditions.check_second_new_field]
      type = "check_fields"
      "second_new_field.equals" = "and not this"

[[tests]]
  name = "another failing test"

  [tests.input]
    insert_at = "foo"
    value = "also this doesnt matter"

  [[tests.outputs]]
    extract_from = "foo"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "message.equals" = "also this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [tests.outputs.conditions.check_new_field]
      type = "check_fields"
      "second_new_field.equals" = "nope not this"
      "third_new_field.equals" = "and not this"
      "message.equals" = "also this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_ne!(tests[0].run(), Vec::<String>::new());
        assert_ne!(tests[1].run(), Vec::<String>::new());
        // TODO: The json representations are randomly ordered so these checks
        // don't always pass:
        /*
                assert_eq!(tests[0].run(), vec![
        r#"check transform 'bar' failed conditions: [ check_second_new_field, check_new_field ], payloads (encoded in JSON format):
          {"second_new_field":"also a string value","message":"nah this doesnt matter"}
        "#.to_owned(),
                ]);
                assert_eq!(tests[1].run(), vec![
        r#"check transform 'baz' failed conditions: [ check_new_field ], payloads (encoded in JSON format):
          {"message":"also this doesnt matter","second_new_field":"also a string value","new_field":"string value"}
        "#.to_owned(),
                ]);
                */
    }
}
