# RFC 14914 - 2023-01-16 - Grok rules through alias source files

Grok rules can reference a set of aliases, given in the `alias` list in the Vector configuration. However,
it is not possible to reference external grok rules/templates. This RFC means to address the latter and
make it easier to maintain grok rules outside of the Vector config.

1. Allow the Grok config to load Grok rules from a set of files.

## Context


## Cross cutting concerns

- https://github.com/vectordotdev/vector/pull/14914

## Scope

### In scope

- [ ] keyword to list files in the config
- [ ] load grok patterns from said file(s)
- [ ] add grok aliases to the existing set of aliases (possibly with checks for duplicates)

### Out of scope

Everything that is not in scope :)

## Pain

- Maintaining a (large) set of aliases that can be referenced from Grok rules, as it can be hard to hardcode these in the Vector config file.

## Proposal

### User Experience

- I have added a new optional keyword to the Grok Parser, which allows specifying a list of files in the Grok configuration part. These files are read and the aliases in them are added to the `aliases` list. They are then processed further using the existing code. The files are expected to be in JSON format, with a list of aliases in each file.
- The proposed change(s) do not break any backwards compatibility, we merely extend the ways in which extra aliases can be defined and made available to the Grok engine.


### Implementation

- There is a new keyword `alias_sources` added to the Grok Parser, along with a new Error type.

```rust
Parameter {
    keyword: "alias_sources",
    kind: kind::ARRAY,
    required: false,
}
```

```rust
InvalidAliasSource {
        path
    } => vec![
        Label::primary(
            format!(r#"invalid source at "{}""#, path.display()),
            Span::default(),
        ),
    ]
```

The latter encapsulates the PathBuf for the faulty file. For each file listed in the `alias_sources` config, the contents of said file should be a JSON list, which after reading are added to the previously defined `aliases` list that was specified in the Grok config.

## Rationale

- First of all, it removes the need to keep all Grok aliases inside the Vector config file. This means that it becomes simpler to generate the config through your favourite cfgmgmt system, as you can specify the location of the file(s) containing the Grok aliases, rather than the aliases themselves. This also means your config is no longer dependent on the set of aliases themselves and these can therefore be developed separately.
- Users having a large set of Grok rules that are not present in the standard set, will need to list them all in the config, which is error prone, may duplicate work, and in general does not promote a clean separation of concerns.
- It facilitates use of in-house developed Grok rules, maintaining and sharing said rules outside of Vector.

## Drawbacks

- The file format is (for now) fixed as JSON, which may not be a good fit for everybody.
- Supporting multiple file formats would require format-specific readers (I think) to convert the data from the file into the key-value pair we need in the alias map.

## Prior Art

I have no idea if there is prior work that tackles this.

## Alternatives

- List all the aliases in the Vector Config. This is unfeasible for a large set of aliases, and cannot be maintained properly.
- Not doing this is not an option for my use case. I have a set of files with Grok patterns that I need to use in the Grok engine. I cannot duplicate them inside the config, since we may accept PRs to the grok pattern repo that I would then need to port back into thet vector config deployment.

## Outstanding Questions

- Should we have a dedicated type in the VRL language to support files on the FS?
- How to address the file format.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [x] Submit a PR with spike-level code _roughly_ demonstrating the change. (#14914)
- [ ] VRL support for files.

Note: This can be filled out during the review process.

## Future Improvements

- I would propose to support different file formats in a later stage.
- Check for duplicate aliases, and define default behaviour (overwrite, ignore, panic)


