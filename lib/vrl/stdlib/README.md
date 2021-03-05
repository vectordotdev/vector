# Vector Remap Language (VRL) Standard Library

This crate contains all VRL functions written for Vector and its supporting
libraries.

## Usage

If your project needs access to all provided functions, use:

```toml
vrl-stdlib = "0.1"
```

If instead, you need a subset of functions, use:

```toml
[dependencies.vrl-stdlib]
version = "0.1"
default-features = false
features = [
    "assert",
    "parse_json",
    # ...
]
```

## Development

To add a new function, do the following:

1. make sure an issue exists to add the relevant function and the issue is
   approved
2. `cp src/parse_json.rs src/my_function.rs`
3. search and replace `ParseJson` for `MyFunction` and `parse_json` for
   `my_function`
4. add relevant feature flags to `Cargo.toml`
5. add relevant `mod` and `use` statements to `src/lib.rs`
6. update tests to define the expected outcome
7. update the relevant `Function` and `Expression` trait implementations to
   match the test expectations
8. create a PR, linking to the original issue
9. ...
10. profit!
