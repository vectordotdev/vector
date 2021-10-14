# RFC 958 - 2021-10-12 - Automatic configuration namespacing

The RFC covers the ability to implicitly namespace Vector configuration based on the configuration directory structure. This provides an easy mechanism for organizing large Vector configuration, something that will become more pronounced as Vector introduces the upcoming Pipelines feature.

## Context

- [Vector Pipelines 2.0](https://docs.google.com/document/d/19L5p-kqvROkygDy9t21nV9EOmxgb_DDbsvoV65ixrk0/edit?usp=sharing)

## Cross cutting concerns

None

## Scope

### In scope

- Implicit namespacing based on Vector's configuration directory structure
- How and where configuration is loaded and namespaced

### Out of scope

- How components configuration should work
- How components should work
- Advanced configuration templating tactics (explicitly including files, etc)
- Multiple configuration directories

## Pain

As Vector evolves and introduces configuration-heavy functionality, like the aggregator role, and the upcoming Pipelines feature, the amount of configuration necessary to program Vector grows large. The ability to organize Vector across multiple files is non-obvious and includes a heavy amount of boilerplate, making the configuration difficult for collaboration and navigation.

## Proposal

To solve for above, we'd like to introduce implicit configuration namespacing based on Vector's configuration directory structure. This aligns the community behind an opinionated method for organizing Vector's configuration, making it easy for users to split up their configuration files and collaborate with others on their team.
### User Experience

- When loading Vector's configuration using `--config-dir` (let say `--config-dir /etc/vector`), every type of component (`sources`, `transforms`, `sinks` but also `enrichment_tables` and `tests`) can be declared in subfolders, in separate files, with there filenames being their component ID.

```toml
# /etc/vector/vector.toml
[sinks.foo]
type = "anything"
```

can become

```toml
# /etc/vector/sinks/foo.toml
type = "anything"
```

- Any duplicate component ID (like `sinks/foo.toml` and `sinks/foo.json`) will error.
- If Vector's configuration is loaded using a specific file (`--config /etc/vector/vector.toml` for example), Vector will keep its default behavior and only load this file.

### Implementation

- [When loading the configuration from a directory](https://github.com/vectordotdev/vector/blob/v0.17.0/src/config/loading.rs#L150), instead of only considering the files in that directory, Vector will check if the folders `sources`, `transforms`, `sinks`, `enrichment_tables` and `tests` exist, load each file in those folders and merge the components with the current configuration.

```rust
fn load_builder_from_dir(path: &Path) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
  let mut builder = ConfigBuilder::default();
  let mut errors = Vec::new();
  for child in path.children() {
    if child.is_dir() {
      match child.name() {
        // same with transforms, sources, tests, enrichment_tables
        "sinks" => load_sinks_from_dir(child, &mut builder, &mut errors),
        other => tracing::debug!("ignoring folder {}", other),
      }
    } else {
      load_builder_from_file(child, &mut builder, &mut errors);
    }
  }
}

// same with transforms, sources, tests, enrichment_tables
fn load_sinks_from_dir(path: &Path, builder: &mut ConfigBuilder, errors: &mut Vec<String>) {
  for child in path.children() {
    if child.is_file() {
      match load_sink_from_file(child) {
        Ok(sink) => builder.add_sink(child.name(), sink.inputs, sink.inner),
        Err(msg) => errors.push(msg),
      };
    }
  }
}
```

## Rationale

Why is this change worth it?

- The new pipeline 2.0 will require such a feature in order to to load the transforms and the inner pipelines.
- This would allow to an admin to allow users to write the files in the `transforms` folder and not read the files in `sinks` folder.

How does this position us for success in the future?

- Pipeline 2.0 will rely on splitting the configuration file into compound transforms. Splitting the configuration that way will allow to have a dedicated folder or file for the definition of those transforms.

> What is the impact of not doing this?

## Drawbacks

Why should we not do this?

- Preparing for Pipeline 2.0 implementation.

What kind on ongoing burden does this place on the team?

- This only changes the configuration loading and therefore won't imply anything on Vector performance.

## Prior Art

- [Datadog Agent](https://docs.datadoghq.com/agent/guide/agent-configuration-files/?tab=agentv6v7)
- [Ansible Playbook syntax](https://docs.ansible.com/ansible/latest/user_guide/playbooks_intro.html#playbook-syntax)
- [Logstash configuration files](https://www.elastic.co/guide/en/logstash/current/config-setting-files.html)
- [Fluentd configuration files](https://docs.fluentd.org/configuration/config-file)

## Outstanding Questions

- Should Vector load `--config-dir /etc/vector` by default instead of loading `--config /etc/vector/vector.toml` to handle subfolders out of the box?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Update the loading process to load components to implement the [strategy design pattern](https://rust-unofficial.github.io/patterns/patterns/behavioural/strategy.html).
- [ ] Load `transforms` from subfolder
- [ ] Load `sinks` from subfolder
- [ ] Load `sources` from subfolder
- [ ] Load `enrichment_tables`  from subfolder
- [ ] Load `tests` from subfolder

Note: This can be filled out during the review process.

## Future Improvements

- Add the Pipeline 2.0 feature
