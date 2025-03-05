<!--
  Your PR title must conform to the conventional commit spec:
  https://www.conventionalcommits.org/en/v1.0.0/

  <type>(<scope>)!: <description>

  * `type` = chore, enhancement, feat, fix, docs, revert
  * `!` = OPTIONAL: signals a breaking change
  * `scope` = Optional when `type` is "chore" or "docs", available scopes https://github.com/vectordotdev/vector/blob/master/.github/workflows/semantic.yml#L31
  * `description` = short description of the change

Examples:

  * enhancement(file source): Add `sort` option to sort discovered files
  * feat(new source): Initial `statsd` source
  * fix(file source): Fix a bug discovering new files
  * chore(external docs): Clarify `batch_size` option
-->

## Summary
<!-- Please provide a brief summary about what this PR does.
This should help the reviewers give feedback faster and with higher quality. -->

## Change Type
- [ ] Bug fix
- [ ] New feature
- [ ] Non-functional (chore, refactoring, docs)
- [ ] Performance

## Is this a breaking change?
- [ ] Yes
- [ ] No

## How did you test this PR?
<!-- Please describe your testing plan here.
Sharing information about your setup and the Vector configuration(s) you used (when applicable) is highly recommended.
Providing this information upfront will facilitate a smoother review process. -->

## Does this PR include user facing changes?

- [ ] Yes. Please add a changelog fragment based on our [guidelines](https://github.com/vectordotdev/vector/blob/master/changelog.d/README.md).
- [ ] No. A maintainer will apply the "no-changelog" label to this PR.

## Checklist
- [ ] Please read our [Vector contributor resources](https://github.com/vectordotdev/vector/tree/master/docs#getting-started).
  - `make check-all` is a good command to run locally. This check is
    defined [here](https://github.com/vectordotdev/vector/blob/1ef01aeeef592c21d32ba4d663e199f0608f615b/Makefile#L450-L454). Some of these
    checks might not be relevant to your PR. For Rust changes, at the very least you should run:
    - `cargo fmt --all`
    - `cargo clippy --workspace --all-targets -- -D warnings`
    - `cargo nextest run --workspace` (alternatively, you can run `cargo test --all`)
- [ ] If this PR introduces changes Vector dependencies (modifies `Cargo.lock`), please
  run `dd-rust-license-tool write` to regenerate the [license inventory](https://github.com/vectordotdev/vrl/blob/main/LICENSE-3rdparty.csv) and commit the changes (if any). More details [here](https://crates.io/crates/dd-rust-license-tool).

## References

<!-- Please list any issues closed by this PR. -->

<!--
- Closes: <issue link>
-->

<!-- Any other issues or PRs relevant to this PR? Feel free to list them here. -->
