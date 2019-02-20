# Router

## Development

To start, you'll need `rustup` installed, along with the `rustfmt-preview` and
`clippy-preview` components installed.

For testing, I use the `flog` tool (`brew install flog`) to generate a 100MB
file of sample data used by the test harness:

```
$ flog --bytes $((100 * 1024 * 1024)) > sample.log
```

My normal development flow goes roughly as follows:

1. `cargo check` as I'm making changes (using `cargo-watch` in a tmux pane makes
   this very convenient)

2. `cargo test && cargo test --features flaky` to check that unit tests are all passing

3. `cargo run` to execute the higher-level test harness that's currently the
   main binary

4. `cargo clippy` to check for any failing lints

5. `cargo fmt` to make sure formatting is correct (I actually have vim setup to
   run rustfmt on save)

Once that's all passing and you're happy with your change, go ahead and commit.
For small, unobtrusive changes, committing to directly to master is fine. For
anything that merits more discussion or visibility, committing to a branch and
opening a pull request is preferred. Just use your best judgement and if you're
unsure, open a pull request.
