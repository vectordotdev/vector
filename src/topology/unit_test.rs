use crate::{
    conditions::Condition,
    event::{Event, Value},
    runtime::Runtime,
    topology::config::{TestCondition, TestDefinition, TestInputValue},
    transforms::Transform,
};
use indexmap::IndexMap;
use std::collections::HashMap;

//------------------------------------------------------------------------------

pub struct UnitTestCheck {
    extract_from: String,
    conditions: Vec<Box<dyn Condition>>,
}

pub struct UnitTestTransform {
    transform: Box<dyn Transform>,
    next: Vec<String>,
}

pub struct UnitTest {
    pub name: String,
    input: (String, Event),
    transforms: IndexMap<String, UnitTestTransform>,
    checks: Vec<UnitTestCheck>,
}

//------------------------------------------------------------------------------

fn event_to_string(event: &Event) -> String {
    match event {
        Event::Log(log) => {
            serde_json::to_string(&log.clone().unflatten()).unwrap_or_else(|_| "{}".into())
        }
        Event::Metric(metric) => serde_json::to_string(&metric).unwrap_or_else(|_| "{}".into()),
    }
}

fn events_to_string(name: &str, events: &Vec<Event>) -> String {
    if events.len() > 1 {
        format!(
            "  {}s:\n    {}",
            name,
            events
                .iter()
                .map(|e| event_to_string(e))
                .collect::<Vec<_>>()
                .join("\n    ")
        )
    } else {
        events
            .first()
            .map(|e| format!("  {}: {}", name, event_to_string(e)))
            .unwrap_or(format!("  no {}", name))
    }
}

fn walk(
    node: &str,
    mut inputs: Vec<Event>,
    transforms: &mut IndexMap<String, UnitTestTransform>,
    aggregated_results: &mut HashMap<String, (Vec<Event>, Vec<Event>)>,
) {
    let mut results = Vec::new();
    let mut targets = Vec::new();

    if let Some(target) = transforms.get_mut(node) {
        for input in inputs.clone() {
            target.transform.transform_into(&mut results, input);
        }
        targets = target.next.clone();
    }

    for child in targets {
        walk(&child, results.clone(), transforms, aggregated_results);
    }

    if let Some((mut e_inputs, mut e_results)) = aggregated_results.remove(node) {
        inputs.append(&mut e_inputs);
        results.append(&mut e_results);
    }
    aggregated_results.insert(node.into(), (inputs, results));
}

impl UnitTest {
    // Executes each test and provides a tuple of inspections and error lists.
    pub fn run(&mut self) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let mut inspections = Vec::new();
        let mut results = HashMap::new();

        walk(
            &self.input.0,
            vec![self.input.1.clone()],
            &mut self.transforms,
            &mut results,
        );

        for check in &self.checks {
            if let Some((inputs, outputs)) = results.get(&check.extract_from) {
                if check.conditions.is_empty() {
                    inspections.push(format!(
                        "check transform '{}' payloads (events encoded as JSON):\n{}\n{}",
                        check.extract_from,
                        events_to_string("input", inputs),
                        events_to_string("output", outputs),
                    ));
                    continue;
                }
                let failed_conditions = check
                    .conditions
                    .iter()
                    .enumerate()
                    .flat_map(|(i, cond)| {
                        outputs
                            .iter()
                            .enumerate()
                            .filter_map(|(j, e)| {
                                cond.check_with_context(e).err().map(|err| {
                                    if outputs.len() > 1 {
                                        format!("condition[{}], payload[{}]: {}", i, j, err)
                                    } else {
                                        format!("condition[{}]: {}", i, err)
                                    }
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                if !failed_conditions.is_empty() {
                    errors.push(format!(
                        "check transform '{}' failed conditions:\n  {}\npayloads (events encoded as JSON):\n{}\n{}",
                        check.extract_from,
                        failed_conditions.join("\n  "),
                        events_to_string("input", inputs),
                        events_to_string("output", outputs),
                    ));
                }
            } else {
                errors.push(format!(
                    "check transform '{}' failed: received zero resulting events.",
                    check.extract_from,
                ));
            }
        }

        (inspections, errors)
    }
}

//------------------------------------------------------------------------------

fn links_to_a_leaf(
    target: &str,
    leaves: &IndexMap<String, ()>,
    link_checked: &mut IndexMap<String, bool>,
    transform_outputs: &IndexMap<String, IndexMap<String, ()>>,
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
    leaves: &IndexMap<String, ()>,
    transform_outputs: &mut IndexMap<String, IndexMap<String, ()>>,
) {
    let mut link_checked: IndexMap<String, bool> = IndexMap::new();

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
    let rt = Runtime::single_threaded().unwrap();
    let mut errors = vec![];

    // Build input event.
    let input_event = match definition.input.type_str.as_ref() {
        "raw" => match definition.input.value.as_ref() {
            Some(v) => Event::from(v.clone()),
            None => {
                errors.push("input type 'raw' requires the field 'value'".to_string());
                Event::from("")
            }
        },
        "log" => {
            if let Some(log_fields) = &definition.input.log_fields {
                let mut event = Event::from("");
                for (path, value) in log_fields {
                    let value: Value = match value {
                        TestInputValue::String(s) => s.as_bytes().into(),
                        TestInputValue::Boolean(b) => (*b).into(),
                        TestInputValue::Integer(i) => (*i).into(),
                        TestInputValue::Float(f) => (*f).into(),
                    };
                    event.as_mut_log().insert(path.to_owned(), value);
                }
                event
            } else {
                errors.push("input type 'log' requires the field 'log_fields'".to_string());
                Event::from("")
            }
        }
        "metric" => {
            if let Some(metric) = &definition.input.metric {
                Event::Metric(metric.clone())
            } else {
                errors.push("input type 'log' requires the field 'log_fields'".to_string());
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
    let mut transform_outputs: IndexMap<String, IndexMap<String, ()>> = config
        .transforms
        .iter()
        .map(|(k, _)| (k.clone(), IndexMap::new()))
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

    let mut leaves: IndexMap<String, ()> = IndexMap::new();
    definition.outputs.iter().for_each(|o| {
        leaves.insert(o.extract_from.clone(), ());
    });

    // Reduce the configured transforms into just the ones connecting our test
    // target with output targets.
    reduce_transforms(&definition.input.insert_at, &leaves, &mut transform_outputs);

    // Build reduced transforms.
    let mut transforms: IndexMap<String, UnitTestTransform> = IndexMap::new();
    for (name, transform_config) in &config.transforms {
        if let Some(outputs) = transform_outputs.remove(name) {
            match transform_config.inner.build(rt.executor()) {
                Ok(transform) => {
                    transforms.insert(
                        name.clone(),
                        UnitTestTransform {
                            transform,
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
    let checks = definition
        .outputs
        .iter()
        .map(|o| {
            let mut conditions: Vec<Box<dyn Condition>> = Vec::new();
            for (index, cond_conf) in o
                .conditions
                .as_ref()
                .unwrap_or(&Vec::new())
                .iter()
                .enumerate()
            {
                match cond_conf {
                    TestCondition::Embedded(b) => match b.build() {
                        Ok(c) => {
                            conditions.push(c);
                        }
                        Err(e) => {
                            errors.push(format!(
                                "failed to create test condition '{}': {}",
                                index, e,
                            ));
                        }
                    },
                    TestCondition::String(_s) => {
                        errors.push(format!(
                            "failed to create test condition '{}': condition references are not yet supported",
                            index
                        ));
                    }
                }
            }
            UnitTestCheck {
                extract_from: o.extract_from.clone(),
                conditions,
            }
        })
        .collect();

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(UnitTest {
            name: definition.name.clone(),
            input: (definition.input.insert_at.clone(), input_event),
            transforms,
            checks,
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
    [[tests.outputs.conditions]]
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
    [[tests.outputs.conditions]]
      type = "check_fields"
      "message.equals" = "not this"

  [[tests.outputs]]
    extract_from = "quz"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "message.equals" = "not this"

[[tests]]
  name = "broken test 2"

  [tests.input]
    insert_at = "nope"
    value = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "quz"
    [[tests.outputs.conditions]]
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
    [[tests.outputs.conditions]]
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
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "bar"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "third_new_field.equals" = "also also a string value"
      "message.equals" = "nah this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "message.equals" = "this is the message"
      "bool_val.eq" = true
      "int_val.eq" = 5
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
      kind = "incremental"
      name = "foometric"
      [tests.input.metric.tags]
        tagfoo = "valfoo"
      [tests.input.metric.value]
        type = "counter"
        value = 100.0

  [[tests.outputs]]
    extract_from = "foo"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "tagfoo.equals" = "valfoo"
      "new_tag.eq" = "new value added"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "third_new_field.equals" = "also also a string value"
      "message.equals" = "nah this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also a string value"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "string value"
      "second_new_field.equals" = "also also a string value"
      "message.equals" = "nah this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_eq!(tests[0].run().1, Vec::<String>::new());
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
    [[tests.outputs.conditions]]
      type = "check_fields"
      "message.equals" = "nah this doesnt matter"

  [[tests.outputs]]
    extract_from = "bar"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "message.equals" = "not this"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "second_new_field.equals" = "and not this"

[[tests]]
  name = "another failing test"

  [tests.input]
    insert_at = "foo"
    value = "also this doesnt matter"

  [[tests.outputs]]
    extract_from = "foo"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "message.equals" = "also this doesnt matter"

  [[tests.outputs]]
    extract_from = "baz"
    [[tests.outputs.conditions]]
      type = "check_fields"
      "second_new_field.equals" = "nope not this"
      "third_new_field.equals" = "and not this"
      "message.equals" = "also this doesnt matter"
      "#,
        )
        .unwrap();

        let mut tests = build_unit_tests(&config).unwrap();
        assert_ne!(tests[0].run().1, Vec::<String>::new());
        assert_ne!(tests[1].run().1, Vec::<String>::new());
        // TODO: The json representations are randomly ordered so these checks
        // don't always pass:
        /*
                assert_eq!(
                    tests[0].run().1,
                    vec![r#"check transform 'bar' failed conditions:
          condition[0]: predicates failed: [ message.equals: 'not this' ]
          condition[1]: predicates failed: [ second_new_field.equals: 'and not this' ]
        payloads (JSON encoded):
          input: {"message":"nah this doesnt matter"}
          output: {"message":"nah this doesnt matter","second_new_field":"also a string value"}"#.to_owned(),
                    ]);
                assert_eq!(
                    tests[1].run().1,
                    vec![r#"check transform 'baz' failed conditions:
          condition[0]: predicates failed: [ second_new_field.equals: 'nope not this', third_new_field.equals: 'and not this' ]
        payloads (JSON encoded):
          input: {"second_new_field":"also a string value","message":"also this doesnt matter"}
          output: {"third_new_field":"also also a string value","second_new_field":"also a string value","message":"also this doesnt matter"}"#.to_owned(),
                    ]);
                */
    }
}
