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

- When loading Vector's configuration using `--config-dir` (let say `--config-dir /etc/vector`), it will look in every subfolder for any component configuration file, with their filenames being their component ID.

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

- Vector will only consider the files with the `yml`, `yaml`, `json`, or `toml` extensions of throw an error.
- Any duplicate component ID (like `sinks/foo.toml` and `sinks/foo.json`) will error.
- If Vector's configuration is **not** loaded using a specific configuration folder (`--config-dir /etc/vector` for example), Vector will keep its default behavior and only load the specified configuration file.
- If Vector encounters a hidden file or a hidden folder (name starting with a `.`, like `/etc/vector/.data` or `/etc/vector/.foo.toml`), the file/folder will be ignored.
- If Vector encounters a folder (like `/etc/vector/foo`) with a name that doesn't refer to a component type (like `sources`, `transforms`, `sinks`, `enrichment_tables`, `tests`), an error will be thrown.
- If a component file (like `/etc/vector/sinks/foo.toml`) doesn't have a proper sink configuration structure, Vector will error.

### Implementation

- [When loading the configuration from a directory](https://github.com/vectordotdev/vector/blob/v0.17.0/src/config/loading.rs#L150), instead of only considering the files in that directory, Vector will consider the subfolders, load each file in those folders and merge the components with the current configuration.

```rust
fn load_builder_from_dir(path: &Path) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
  let mut builder = ConfigBuilder::default();
  let mut errors = Vec::new();
  for child in path.children() {
    if child.is_dir() {
      match child.name() {
        // same with other component types like transforms, sources, tests, enrichment_tables
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

- Having this kind of feature for the new pipeline 2.0 will make it easier to load the transforms and the inner pipelines. It will also make it much nicer to use across multiple teams.
- This would allow to an admin to allow users to write the files in the `transforms` folder and not read the files in `sinks` folder.

How does this position us for success in the future?

- Pipeline 2.0 will rely on splitting the configuration file into compound transforms. Splitting the configuration that way will allow to have a dedicated folder or file for the definition of those transforms.

## Drawbacks

### Why should we not do this?

This feature is a nice-to-have for users with large configuration files, but it is not required for any upcoming development. The new Pipelines feature can still function with a single configuration file, but it exacerbates the problem stated in the [pain](#pain).

### What kind on ongoing burden does this place on the team?

Very little. This will have minimal impact to the Vector codebase. Configuration will be loaded and built in one step  before configuration is validated. I don't foresee this introduces any meaningful maintenance burden on the team.

## Prior Art

- [Ansible Playbook syntax](https://docs.ansible.com/ansible/latest/user_guide/playbooks_intro.html#playbook-syntax)

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
