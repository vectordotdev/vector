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

## Notes
- Please read our [Vector contributor resources](https://github.com/vectordotdev/vector/tree/master/docs#getting-started).
- Do not hesitate to use `@vectordotdev/vector` to reach out to us regarding this PR.
- The CI checks run only after we manually approve them.
  - We recommend adding a `pre-push` hook, please see [this template](https://github.com/vectordotdev/vector/blob/master/CONTRIBUTING.md#Pre-push).
  - Alternatively, we recommend running the following locally before pushing to the remote branch:
    - `cargo fmt --all`
    - `cargo clippy --workspace --all-targets -- -D warnings`
    - `cargo nextest run --workspace` (alternatively, you can run `cargo test --all`)
    - `./scripts/check_changelog_fragments.sh`
- After a review is requested, please avoid force pushes to help us review incrementally.
  - Feel free to push as many commits as you want. They will be squashed into one before merging.
  - For example, you can run `git merge origin master` and `git push`.
- If this PR introduces changes Vector dependencies (modifies `Cargo.lock`), please
  run `cargo vdev build licenses` to regenerate the [license inventory](https://github.com/vectordotdev/vrl/blob/main/LICENSE-3rdparty.csv) and commit the changes (if any). More details [here](https://crates.io/crates/dd-rust-license-tool).

## References

<!-- Please list any issues closed by this PR. -->

<!--
- Closes: <issue link>
-->

<!-- Any other issues or PRs relevant to this PR? Feel free to list them here. -->
