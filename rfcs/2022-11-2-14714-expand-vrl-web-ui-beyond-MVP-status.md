# RFC 14714 - 2022-11-02 - Expand VRL web UI beyond MVP status

[The current MVP](https://playground.vrl.dev/) for the VRL web
playground utilizes WASM and runs the majority of the VRL stdlib,
[except for six functions](https://github.com/vectordotdev/vector/issues?q=wasm+compatible+label%3A%22vrl%3A+playground%22+is%3Aopen).
Users can run VRL programs, share their programs online, and try
sample event inputs. However, we would like to expand the MVP
features which would require more planning and decision making.
An outline of what we would like to implement can be found in the
Context section below.

## Context

Epic covering the progress of the playground:

- https://github.com/vectordotdev/vector/issues/14652

List of relevant stretch goals:

- Add syntax highlighting
- Add keyboard shortcut support
- Add advanced IDE features (auto-completion, inline docs, etc...)
- Add button to convert tested program to working Vector config.toml file
- Support multiple VRL (Vector) versions

## Cross cutting concerns

- https://github.com/vectordotdev/vector/issues/14714.

## Scope

### In scope

- Leveraging a web framework that would facilitate a maintainable playground.
- Styling the playground to abide by Vector style guides.
- Adding UI components that support stretch goals such as Dialogs, ToolBars, MenuItems, etc.
- Adding CI steps to release `/lib/vrl/web-playground/public` for running VRL under different versions of Vector.
- Migrating current MVP features to new UI.
- Setting up the long-term repo for the web playground.

### Out of scope

- Creating a language server for VRL.

## Pain

The alternatives are covered in a section below but the main
concerns have to do with.

- Maintainability. We must ensure other engineers can build on top of the playground in the long term.

- Timeline. There's six weeks left in Jonathan's internship

- Lack of customizeability, current alternatives will provide bottlenecks in the long term future for adding more features

## Proposal

### User Experience

- The expansion of the MVP UI is necessary to create more robust features that tightly integrates with VRL and Vector so users can get a clear

### Implementation

The new UI will set up the building blocks for a robust
playground experience. Live deploy, auto completion, in line docs,
and more desired features will be quicker to implement in a

- When possible, demonstrate with pseudo code not text.
- Be specific. Be opinionated. Avoid ambiguity.

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

There were other approaches to expanding the MVP but were not
considered due to concerns about maintainability, scope,

- Utilize graphiql as a front-end library and add run_vrl

This would buy us a lot of front end componenets, but after
diving deep into the repository, some dependencies must be
re-written, and stripped from the context of graphql. This means
that the project will likely cause tech debt in the long term
future. Requiring engineers to ramp up to a mono-repo, familiarize
with a lot of css, familiarize with the build tools used, etc.

Jean estimates a week or two of effort to strip down the graphiql
mono repo to solely use code-mirror instead of code-mirror-graphql
the difference between these two dependencies the latter has tight
coupiling of syntax highlighting and autocompletion logic for
graphql schemas.

We already have an implementation of running VRL within the graphiql
context but as mentioned earlier in the RFC, would likely introduce
unnecessary tech-debt in the long term, Steve mentioned the
graphiql-react library seems to be an implementation of another
UI library, which we should at least expirement with first before
solely utilizing graphiql.

- Utilize a rust web framework
This would allow us to write a website in rust, fitting for the
context of vector, and would allow me to ramp up with rust, serving
as training for me to have more transferable skills to eventually
join vector full time.

This introduces a lot of overhead and seems like would require too
much effort ramping up to frameworks as Jean mentioned this is
not a task to take lightly and has previous experience developing
in a rust web framework.

-

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Design UI layout in Figma with toolbar, side panels, editors, header, footer
- [ ] Take UI design to real app with React components
- [ ] Migrate MVP functionality to new UI
- [ ] Implement word completion (not intellisense) on code editors for WASM supported VRL stdlib functions
- [ ] Implement keyboard shortcut to run VRL
- [ ] Add export to Vector toml file feature

## Future Improvements

- Adding performance monitoring on VRL programs, so users can see how to optimize their programs
- Adding live manipulation of vector instances
