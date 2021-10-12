# RFC 958 - 2021-10-12 - Automatic namespacing

Or how to load the sink `foo` from `${CONFIG_DIR}/sinks/foo.toml`.
This can be seen as a prior work regarding the pipeline feature.

## Context

- [Multiple pipelines RFC](rfcs/2021-07-19-8216-multiple-pipelines.md)

## Cross cutting concerns

- Link to any ongoing or future work relevant to this change.

## Scope

### In scope

- How configuration files could be splitted into folders
- How configuration should be loaded

### Out of scope

- How components configuration should work



- List work that is completely out of scope. Use this to keep discussions focused. Please note the "future changes" section at the bottom.

## Pain

- The actual configuration files can become big and it can become hard to follow when looking for components.



- What internal or external *pain* are we solving?
- Do not cover benefits of your change, this is covered in the "Rationale" section.

## Proposal

### User Experience

- When loading Vector's configuration using `--config-dir` (let say `--config-dir /etc/vector`), every type of component (`sources`, `transforms`, `sinks` but also `enrichment_tables` and `tests`) can be declared in subfolders, in seperate files, with there filenames being their component ID.

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

- Explain your change as if you were describing it to a Vector user. We should be able to share this section with a Vector user to solicit feedback.
- Does this change break backward compatibility? If so, what should users do to upgrade?

### Implementation

- [When loading the configuration from a directory](https://github.com/vectordotdev/vector/blob/v0.17.0/src/config/loading.rs#L150), instead of only considering the files in that directory, Vector will check if the folders `sources`, `transforms`, `sinks`, `enrichment_tables` and `tests` exist, load each files in those folders and merge the components with the current configuration.

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

What is the impact of not doing this?



How does this position us for success in the future?

- Pipeline 2.0 will rely on splitting the configuration file into compound transforms. Splitting the configuration that way will allow to have a dedicated folder or file for the definition of those transforms.

## Drawbacks

Why should we not do this?

- Preparing for Pipeline 2.0 implementation.

What kind on ongoing burden does this place on the team?

- This only changes the configuration loading and therefore won't imply anything on Vector performance.

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

## Outstanding Questions

- Should Vector load `--config-dir /etc/vector` by default instead of loading `--config /etc/vector/vector.toml` in order to handle subfolders out of the box?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Update the loading process to load components in order to implement the [strategy design pattern](https://rust-unofficial.github.io/patterns/patterns/behavioural/strategy.html).
- [ ] Load `transforms` from subfolder
- [ ] Load `sinks` from subfolder
- [ ] Load `sources` from subfolder
- [ ] Load `enrichment_tables`  from subfolder
- [ ] Load `tests` from subfolder

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.
