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

- Lack of customizability, current alternatives will provide bottlenecks in the long term future for adding more features

## Proposal

### User Experience

- The expansion of the MVP UI is necessary to create more robust features that tightly integrates with VRL and Vector so users can get a clear picture of what VRL can do.

### Implementation

The new UI will set up the building blocks for a robust
playground experience. Live deploy, auto completion, in line docs,
and more desired features will be possible to implement after
the building blocks of the site are made. We should be able
to have components for tabs, editors, toolbars, and more.
Each with the possibility to expand with features that will
accomplish the stretch goals.

- Live deploy => we would likely require a robust component for the editors, one that handles state and context so we can add additional logic to expand to live deployment to vector instances.
- Auto completion => we can investigate adding the AST onto the monaco editors and keeping that logic in a separate module similar to how graphiql created codemirror-graphql we can create monaco-vrl.
- UI building blocks => will allow us to implement tabs, docs, editor buttons (sharing, copy code, etc), toolbars

## Rationale

- Why is this change worth it?
This change will allow us to expand the current playground to a more long-term maintainable project.

- What is the impact of not doing this?
The impact will be a less organized code-base (if using graphiql) or a more complex project for what we need
the playground to accomplish (going the rust web-framework route).

- How does this position us for success in the future?
This will allow us to develop a nice onboarding experience for users who want to see what VRL has to offer,
making it an easier sell for users, or at the very least an easier demo to potential customers.

## Drawbacks

- Why should we not do this?
We should not do this if the MVP playground is already enough and no longer needs more iteration,
if we do not forsee making the playground more complex than it already is in the MVP, then the
effort will not be useful.

- What kind on ongoing burden does this place on the team?
The burden will take place in requiring more front end developers or building up context for
current Vector developers.

## Prior Art

- List prior art, the good and bad.
- MVP Playground - great demonstration of what can be done, but limited in features we would like
- Graphiql-Vector Playground - has more features we want but the codebase is too tightly coupled with graphql

- Why can't we simply use or copy them?
- The above prior art is not sufficient for what we would like to develop in the future, and can be too difficult to maintain long term.

## Alternatives

There were other approaches to expanding the MVP but were not
considered due to concerns about maintainability, scope,

- Utilize graphiql as a front-end library and add run_vrl

This would buy us a lot of front end components, but after
diving deep into the repository, some dependencies must be
re-written, and stripped from the context of graphql. This means
that the project will likely cause tech debt in the long term
future. Requiring engineers to ramp up to a mono-repo, familiarize
with a lot of css, familiarize with the build tools used, etc.

Jean estimates a week or two of effort to strip down the graphiql
mono repo to solely use code-mirror instead of code-mirror-graphql
the difference between these two dependencies the latter has tight
coupling of syntax highlighting and autocompletion logic for
graphql schemas.

We already have an implementation of running VRL within the graphiql
context but as mentioned earlier in the RFC, would likely introduce
unnecessary tech-debt in the long term, Steve mentioned the
graphiql-react library seems to be an implementation of another
UI library, which we should at least experiment with first before
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

- What would observability pipelines team like to see in a VRL playground effort, npm modules?, more vrl-js docs?
- What would customers like to see for the VRL playground?

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
