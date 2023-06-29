This is a checklist to use whilst reviewing code for Vector's sinks.

## Logic

- [ ] Does it work? Do you understand what it is supposed to be doing?
- [ ] Does the retry logic make sense?
- [ ] Are they emitting the correct metrics?
- [ ] Are the tests testing that they are emitting the correct metrics?
- [ ] Are there integration tests?

## Code structure

- [ ] Is it using the sink prelude?
- [ ] Sensible feature flags?
- [ ] Is the code modularized into `mod.rs`, `config.rs`, `sink.rs`,  `request_builder.rs`, `service.rs`
- [ ] Does the code follow our [style guidelines].

## Documentation

- [ ] Look at the doc preview on Netlify. Does it look good? 
- [ ] Is there a `cue` file linking to `base`?
- [ ] Is there a markdown file under `/website/content/en/docs/reference/configuration/sinks/`?
- [ ] Are module comments included in `mod.rs` linking to any relevant areas in the external services documentation?

## Configuration

- [ ] Are TLS settings configurable?
- [ ] Are the Request settings configurable?
- [ ] Proxy settings?
- [ ] Batch settings?

[style guidelines]: https://github.com/vectordotdev/vector/blob/master/STYLE.md