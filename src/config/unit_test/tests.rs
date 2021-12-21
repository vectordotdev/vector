use super::*;
use crate::config::ConfigBuilder;
use indoc::indoc;

#[tokio::test]
async fn parse_no_input() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
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
              type = "vrl"
              source = ""
    "#})
    .unwrap();

    let errs = build_unit_tests(config).await.err().unwrap();
    assert_eq!(
        errs,
        vec![indoc! {r#"
            Failed to build test 'broken test':
              inputs[0]: unable to locate target transform 'foo'"#}
        .to_owned(),]
    );

    let config: ConfigBuilder = toml::from_str(indoc! {r#"
        [transforms.bar]
          inputs = ["foo"]
          type = "add_fields"
          [transforms.bar.fields]
            my_string_field = "string value"

          [[tests]]
            name = "broken test"

          [[tests.inputs]]
            insert_at = "bar"
            value = "nah this doesnt matter"

          [[tests.inputs]]
            insert_at = "foo"
            value = "nah this doesnt matter"

          [[tests.outputs]]
            extract_from = "bar"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = ""
    "#})
    .unwrap();

    let errs = build_unit_tests(config).await.err().unwrap();
    assert_eq!(
        errs,
        vec![indoc! {r#"
            Failed to build test 'broken test':
              inputs[1]: unable to locate target transform 'foo'"#}
        .to_owned(),]
    );
}

#[tokio::test]
async fn parse_no_test_input() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
        [transforms.bar]
          inputs = ["foo"]
          type = "add_fields"
          [transforms.bar.fields]
            my_string_field = "string value"

          [[tests]]
            name = "broken test"

          [[tests.outputs]]
            extract_from = "bar"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = ""
    "#})
    .unwrap();

    let errs = build_unit_tests(config).await.err().unwrap();
    assert_eq!(
        errs,
        vec![indoc! {r#"
            Failed to build test 'broken test':
              must specify at least one input."#}
        .to_owned(),]
    );
}

#[tokio::test]
async fn parse_no_outputs() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
        [transforms.foo]
          inputs = ["ignored"]
          type = "add_fields"
          [transforms.foo.fields]
            my_string_field = "string value"

          [[tests]]
            name = "broken test"

          [tests.input]
            insert_at = "foo"
            value = "nah this doesnt matter"
    "#})
    .unwrap();

    let errs = build_unit_tests(config).await.err().unwrap();
    assert_eq!(
        errs,
        vec![indoc! {r#"
            Failed to build test 'broken test':
              unit test must contain at least one of `outputs` or `no_outputs_from`."#}
        .to_owned(),]
    );
}

#[tokio::test]
async fn parse_broken_topology() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
        [transforms.foo]
          inputs = ["something"]
          type = "add_fields"
          [transforms.foo.fields]
            foo_field = "string value"

        [transforms.nah]
          inputs = ["ignored"]
          type = "add_fields"
          [transforms.nah.fields]
            new_field = "string value"

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
              type = "vrl"
              source = """
                assert_eq!(.message, "not this")
              """

          [[tests.outputs]]
            extract_from = "quz"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.message, "not this")
              """

        [[tests]]
          name = "broken test 2"

          [[tests.inputs]]
            insert_at = "foo"
            value = "nah this doesnt matter"

          [[tests.inputs]]
            insert_at = "nah"
            value = "nah this doesnt matter"

          [[tests.outputs]]
            extract_from = "baz"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.message, "not this")
              """

          [[tests.outputs]]
            extract_from = "quz"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.message, "not this")
              """

        [[tests]]
          name = "successful test"
          [[tests.inputs]]
          insert_at = "foo"
          value = "this does matter"

        [[tests.outputs]]
          extract_from = "foo"
          [[tests.outputs.conditions]]
            type = "vrl"
            source = """
            assert_eq!(.message, "this does matter")
            assert_eq!(.foo_field, "string value")
            """
    "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    let mut errs_broken_test = tests.remove(0).run().await.1;
    errs_broken_test.sort();
    let mut errs_broken_test_2 = tests.remove(0).run().await.1;
    errs_broken_test_2.sort();
    let expected_errs = vec![
        r#"Check transform "baz" failed, no events received. Topology may be disconnected or this transform is missing inputs."#,
        r#"Check transform "quz" failed, no events received. Topology may be disconnected or this transform is missing inputs."#,
    ];
    assert_eq!(errs_broken_test, expected_errs);
    assert_eq!(errs_broken_test_2, expected_errs);
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn parse_bad_input_event() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
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
              type = "vrl"
              source = ""
    "#})
    .unwrap();

    let errs = build_unit_tests(config).await.err().unwrap();
    assert_eq!(
        errs,
        vec![indoc! {r#"
            Failed to build test 'broken test':
              unrecognized input type 'nah', expected one of: 'raw', 'log' or 'metric'"#}
        .to_owned(),]
    );
}

#[tokio::test]
async fn test_success_multi_inputs() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
        [transforms.foo]
          inputs = ["ignored"]
          type = "add_fields"
          [transforms.foo.fields]
            new_field = "string value"

        [transforms.foo_two]
          inputs = ["ignored"]
          type = "add_fields"
          [transforms.foo_two.fields]
            new_field_two = "second string value"

        [transforms.bar]
          inputs = ["foo", "foo_two"]
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

          [[tests.inputs]]
            insert_at = "foo"
            value = "nah this doesnt matter"

          [[tests.inputs]]
            insert_at = "foo_two"
            value = "nah this also doesnt matter"

          [[tests.outputs]]
            extract_from = "foo"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field, "string value")
                assert_eq!(.message, "nah this doesnt matter")
              """

          [[tests.outputs]]
            extract_from = "foo_two"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field_two, "second string value")
                assert_eq!(.message, "nah this also doesnt matter")
              """

          [[tests.outputs]]
            extract_from = "bar"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field, "string value")
                assert_eq!(.second_new_field, "also a string value")
                assert_eq!(.message, "nah this doesnt matter")
              """
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field_two, "second string value")
                assert_eq!(.second_new_field, "also a string value")
                assert_eq!(.message, "nah this also doesnt matter")
              """

          [[tests.outputs]]
            extract_from = "baz"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field, "string value")
                assert_eq!(.second_new_field, "also a string value")
                assert_eq!(.third_new_field, "also also a string value")
                assert_eq!(.message, "nah this doesnt matter")
              """
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field_two, "second string value")
                assert_eq!(.second_new_field, "also a string value")
                assert_eq!(.third_new_field, "also also a string value")
                assert_eq!(.message, "nah this also doesnt matter")
              """
    "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_success() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
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
              type = "vrl"
              source = """
                assert_eq!(.new_field, "string value")
                assert_eq!(.message, "nah this doesnt matter")
              """

          [[tests.outputs]]
            extract_from = "bar"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field, "string value")
                assert_eq!(.second_new_field, "also a string value")
                assert_eq!(.message, "nah this doesnt matter")
              """

          [[tests.outputs]]
            extract_from = "baz"
            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.new_field, "string value")
                assert_eq!(.second_new_field, "also a string value")
                assert_eq!(.third_new_field, "also also a string value")
                assert_eq!(.message, "nah this doesnt matter")
              """
    "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_route() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
          [transforms.foo]
            inputs = ["ignored"]
            type = "route"
              [transforms.foo.route]
              first = '.message == "test swimlane 1"'
              second = '.message == "test swimlane 2"'

          [transforms.bar]
            inputs = ["foo.first"]
            type = "add_fields"
            [transforms.bar.fields]
              new_field = "new field added"

          [[tests]]
            name = "successful route test 1"

            [tests.input]
              insert_at = "foo"
              value = "test swimlane 1"

            [[tests.outputs]]
              extract_from = "foo.first"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                    assert_eq!(.message, "test swimlane 1")
                """

            [[tests.outputs]]
              extract_from = "bar"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                    assert_eq!(.message, "test swimlane 1")
                    assert_eq!(.new_field, "new field added")
                """

          [[tests]]
            name = "successful route test 2"

            [tests.input]
              insert_at = "foo"
              value = "test swimlane 2"

            [[tests.outputs]]
              extract_from = "foo.second"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                    assert_eq!(.message, "test swimlane 2")
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_fail_no_outputs() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
          [transforms.foo]
            inputs = [ "TODO" ]
            type = "filter"
            [transforms.foo.condition]
              type = "vrl"
              source = """
                .not_exist == "not_value"
              """

            [[tests]]
              name = "check_no_outputs"
              [tests.input]
                insert_at = "foo"
                type = "raw"
                value = "test value"

              [[tests.outputs]]
                extract_from = "foo"
                [[tests.outputs.conditions]]
                  type = "vrl"
                  source = """
                    assert_eq!(.message, "test value")
                  """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_fail_two_output_events() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
          [transforms.foo]
            inputs = [ "TODO" ]
            type = "add_fields"
            [transforms.foo.fields]
              foo = "new field 1"

          [transforms.bar]
            inputs = [ "foo" ]
            type = "add_fields"
            [transforms.bar.fields]
              bar = "new field 2"

          [transforms.baz]
            inputs = [ "foo" ]
            type = "add_fields"
            [transforms.baz.fields]
              baz = "new field 3"

          [transforms.boo]
            inputs = [ "bar", "baz" ]
            type = "add_fields"
            [transforms.boo.fields]
              boo = "new field 4"

          [[tests]]
            name = "check_multi_payloads"

            [tests.input]
              insert_at = "foo"
              type = "raw"
              value = "first"

            [[tests.outputs]]
              extract_from = "boo"

              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                  assert_eq!(.baz, "new field 3")
                """

              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                  assert_eq!(.bar, "new field 2")
                """

          [[tests]]
            name = "check_multi_payloads_bad"

            [tests.input]
              insert_at = "foo"
              type = "raw"
              value = "first"

            [[tests.outputs]]
              extract_from = "boo"

              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                    assert_eq!(.baz, "new field 3")
                    assert_eq!(.bar, "new field 2")
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_no_outputs_from() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
          [transforms.foo]
            inputs = [ "ignored" ]
            type = "filter"
            [transforms.foo.condition]
              type = "vrl"
              source = """
                .message == "foo"
              """

          [[tests]]
            name = "check_no_outputs_from_succeeds"
            no_outputs_from = [ "foo" ]

            [tests.input]
              insert_at = "foo"
              type = "raw"
              value = "not foo at all"

          [[tests]]
            name = "check_no_outputs_from_fails"
            no_outputs_from = [ "foo" ]

            [tests.input]
              insert_at = "foo"
              type = "raw"
              value = "foo"
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_no_outputs_from_chained() {
    let config: ConfigBuilder = toml::from_str(indoc! { r#"
          [transforms.foo]
            inputs = [ "ignored" ]
            type = "filter"
            [transforms.foo.condition]
              type = "vrl"
              source = """
                .message == "foo"
              """

          [transforms.bar]
            inputs = [ "foo" ]
            type = "add_fields"
            [transforms.bar.fields]
              bar = "new field"

          [[tests]]
            name = "check_no_outputs_from_succeeds"
            no_outputs_from = [ "bar" ]

            [tests.input]
              insert_at = "foo"
              type = "raw"
              value = "not foo at all"

          [[tests]]
            name = "check_no_outputs_from_fails"
            no_outputs_from = [ "bar" ]

            [tests.input]
              insert_at = "foo"
              type = "raw"
              value = "foo"
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_log_input() {
    let config: ConfigBuilder = toml::from_str(indoc! { r#"
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
                type = "vrl"
                source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.message, "this is the message")
                    assert_eq!(.message, "this is the message")
                    assert!(.bool_val)
                    assert_eq!(.int_val, 5)
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_metric_input() {
    let config: ConfigBuilder = toml::from_str(indoc! { r#"
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
                [tests.input.metric.counter]
                  value = 100.0

            [[tests.outputs]]
              extract_from = "foo"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                    assert_eq!(.tags.tagfoo, "valfoo")
                    assert_eq!(.tags.new_tag, "new value added")
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_success_over_gap() {
    let config: ConfigBuilder = toml::from_str(indoc! { r#"
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
                type = "vrl"
                source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.third_new_field, "also also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_success_tree() {
    let config: ConfigBuilder = toml::from_str(indoc! { r#"
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
                type = "vrl"
                source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                """

            [[tests.outputs]]
              extract_from = "baz"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                    assert_eq!(.new_field, "string value")
                    assert_eq!(.second_new_field, "also also a string value")
                    assert_eq!(.message, "nah this doesnt matter")
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_fails() {
    let config: ConfigBuilder = toml::from_str(indoc! { r#"
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
                type = "vrl"
                source = """
                  assert_eq!(.message, "nah this doesnt matter")
                """

            [[tests.outputs]]
              extract_from = "bar"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                  assert_eq!(.message, "not this")
                """
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                  assert_eq!(.second_new_field, "and not this")
                """

          [[tests]]
            name = "another failing test"

            [tests.input]
              insert_at = "foo"
              value = "also this doesnt matter"

            [[tests.outputs]]
              extract_from = "foo"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                  assert_eq!(.message, "also this doesnt matter")
                """

            [[tests.outputs]]
              extract_from = "baz"
              [[tests.outputs.conditions]]
                type = "vrl"
                source = """
                  assert_eq!(.second_new_field, "nope not this")
                  assert_eq!(.third_new_field, "and not this")
                  assert_eq!(.message, "also this doesnt matter")
                """
      "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn type_inconsistency_while_expanding_transform() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
          [sources.input]
            type = "demo_logs"
            format = "shuffle"
            lines = ["one", "two"]
            count = 5

          [transforms.foo]
            inputs = ["input"]
            type = "compound"
            [[transforms.foo.steps]]
              id = "step1"
              type = "log_to_metric"
              [[transforms.foo.steps.metrics]]
                type = "counter"
                field = "c"
                name = "sum"
                namespace = "ns"
            [[transforms.foo.steps]]
              id = "step2"
              type = "log_to_metric"
              [[transforms.foo.steps.metrics]]
                type = "counter"
                field = "c"
                name = "sum"
                namespace = "ns"

          [sinks.output]
            type = "console"
            inputs = [ "foo.step2" ]
            encoding = "json"
            target = "stdout"
      "#})
    .unwrap();

    let err = crate::config::compiler::compile(config).err().unwrap();
    assert_eq!(
        err,
        vec!["Data type mismatch between foo.step1 (Metric) and foo.step2 (Log)".to_owned()]
    );
}

#[tokio::test]
async fn invalid_name_in_expanded_transform() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
          [sources.input]
            type = "demo_logs"
            format = "shuffle"
            lines = ["one", "two"]
            count = 5

          [transforms.foo]
            inputs = ["input"]
            type = "compound"
            [[transforms.foo.steps]]
              type = "log_to_metric"
              [[transforms.foo.steps.metrics]]
                type = "counter"
                field = "c"
                name = "sum"
                namespace = "ns"
            [[transforms.foo.steps]]
              id = "0"
              type = "log_to_metric"
              [[transforms.foo.steps.metrics]]
                type = "counter"
                field = "c"
                name = "sum"
                namespace = "ns"

          [sinks.output]
            type = "console"
            inputs = [ "foo.0" ]
            encoding = "json"
            target = "stdout"
      "#})
    .unwrap();

    let err = crate::config::compiler::compile(config).err().unwrap();
    assert_eq!(
        err,
        vec![
            "failed to expand transform 'foo': conflicting id found while expanding transform"
                .to_owned()
        ]
    );
}

#[tokio::test]
async fn test_dropped_branch() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
    [transforms.droptest]
      type = "remap"
      inputs = [ "ignored" ]
      drop_on_error = true
      drop_on_abort = true
      reroute_dropped = true
      source = "abort"

    [transforms.another]
      type = "remap"
      inputs = [ "droptest.dropped" ]
      source = """
          .new_field = "a new field"
      """

    [[tests]]
      name = "dropped branch test"
      no_outputs_from = [ "droptest" ]

      [[tests.inputs]]
        type = "log"
        insert_at = "droptest"

        [tests.inputs.log_fields]
          message = "test1"

      [[tests.inputs]]
        type = "log"
        insert_at = "droptest"

        [tests.inputs.log_fields]
          message = "test2"

      [[tests.outputs]]
        extract_from = "droptest.dropped"

        [[tests.outputs.conditions]]
          type = "vrl"
          source = """
              assert_eq!(.message, "test2", "incorrect message")
          """

    [[tests]]
      name = "dropped branch test no_outputs_from on branch (should fail)"
      no_outputs_from = [ "droptest.dropped" ]

      [[tests.inputs]]
        type = "log"
        insert_at = "droptest"

        [tests.inputs.log_fields]
          message = "test1"

    [[tests]]
      name = "dropped branch test failure"
      no_outputs_from = [ "droptest" ]

      [[tests.inputs]]
        type = "log"
        insert_at = "droptest"

        [tests.inputs.log_fields]
          message = "test1"

      [[tests.outputs]]
        extract_from = "droptest.dropped"

        [[tests.outputs.conditions]]
          type = "vrl"
          source = """
              assert_eq!(.message, "bad message", "incorrect message")
          """
  "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}

#[tokio::test]
async fn test_task_transform() {
    let config: ConfigBuilder = toml::from_str(indoc! {r#"
        [transforms.ingress1]
          type = "remap"
          inputs = [ "ignored" ]
          source = '.new_field = "value1"'

        [transforms.ingress2]
          type = "remap"
          inputs = [ "ignored" ]
          source = '.another_new_field = "value2"'

        [transforms.task-transform]
          type = "reduce"
          inputs = [ "ingress1", "ingress2" ]
          group_by = [ "message" ]

        [[tests]]
          name = "task transform test"

          [[tests.inputs]]
            type = "log"
            insert_at = "ingress1"

            [tests.inputs.log_fields]
              message = "test1"

          [[tests.inputs]]
            type = "log"
            insert_at = "ingress2"

            [tests.inputs.log_fields]
              message = "test1"

          [[tests.outputs]]
            extract_from = "task-transform"

            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.message, "test1", "incorrect message")
                assert_eq!(.new_field, "value1", "incorrect value")
                assert_eq!(.another_new_field, "value2", "incorrect value")
              """

        [[tests]]
          name = "task transform test failure"

          [[tests.inputs]]
            type = "log"
            insert_at = "ingress1"

            [tests.inputs.log_fields]
              message = "test1"

          [[tests.inputs]]
            type = "log"
            insert_at = "ingress2"

            [tests.inputs.log_fields]
              message = "different message"

          [[tests.outputs]]
            extract_from = "task-transform"

            [[tests.outputs.conditions]]
              type = "vrl"
              source = """
                assert_eq!(.message, "test1", "incorrect message")
                assert_eq!(.new_field, "value1", "incorrect value")
                assert_eq!(.another_new_field, "value2", "incorrect value")
              """
    "#})
    .unwrap();

    let mut tests = build_unit_tests(config).await.unwrap();
    assert_eq!(tests.remove(0).run().await.1, Vec::<String>::new());
    assert_ne!(tests.remove(0).run().await.1, Vec::<String>::new());
}
