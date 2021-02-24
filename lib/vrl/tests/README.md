# Vector Remap Language Test Harness

This test harness allows us to run a variety of test cases against the latest
build of VRL.

The goal is to make it as simple as possible for other contributors to expand
our test coverage, without the requirement to know anything about the internals
of VRL itself, or the language in which VRL is built (Rust).

## Adding Tests

New tests are added in the [`tests`](./tests) sub-directory. There's an
[`example.vrl`](./tests/example.vrl) file to show the general structure of a
test case.

Each directory inside the test directory has its own documentation to explain
which tests go where.

## Q&A

- **How can I run these tests locally?**

  For now, you need to have [Rust](https://www.rust-lang.org/) installed, and
  then run `cargo run` in this directory.

  You can also use [cargo-watch](https://crates.io/crates/cargo-watch) to
  continuously run the tests as you make changes to VRL or the tests themselves.

- **How does the CLI work?**

  It's fairly basic right now. There's no `--help`, but you can provide
  `--verbose` to see the output of each test regardless if it failed or
  succeeded.

  Note that, when using Cargo, you need to run `cargo run -- --verbose`.

- **Can I add any test I want?**

  Yes! If you submit a test, we might ask you to move it to a different
  directory, rename it, or add a few tweaks, but in general, the more tests the
  better.

- **I don't know how to add a test, but would like to contribute!**

  If the existing documentation isn't clear enough, then feel free to open an
  issue describing what's unclear. We'll help you out in the issue and make sure
  to improve the documentation for future contributors.

- **I've made changes to VRL, resulting in many broken tests!**

  This is an unfortunate — but expected — side-effect of using "UI testing".

  We're planning on adding a `--update-tests` flag to mass-update existing tests
  in one go, but until then, you'll have to update tests manually.

  If you have a large number of failing tests, please open an issue, as it might
  be less work to add the above-mentioned flag, than to update the tests
  manually.
