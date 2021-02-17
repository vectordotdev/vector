# TODO

This document contains a list of features we want to add to the CLI. These will
be filed as issues, once development is a bit further along.

## JSON Input

- support jsonl (e.g. multiple JSON lines) for `--input`

## Output Formatting

- provide `--output-format=simple,json,...` to define the format of the result
- `simple` returns the (current) easy to read format: `{foo: "bar", baz: [0, true]}`
- `json` returns valid JSON output
- might add more if useful
- should default to `json` for maximum compatibility

## Profiling

- add `--profile` flag
- profile performance of TRL scripts
- execution time (total and per expression)
- memory allocations (<https://docs.rs/dhat/0.1.1/dhat/>)

## Documentation

- add `--docs` flag
- should open the TRL documentation in the browser
- add `--examples` flag
- should print a list of common input > program > result examples

## Add Function Support

- currently all TRL functions are implemented in Vector itself
- this means they aren't available in this CLI
- we should move those into a `trl-contrib` library
- this CLI (and Vector) will depend on that library
- this ensures feature parity between Vector and this CLI

## Support WASM Binary

- this one is only partially related to the CLI
- the goal is to compile TRL to wasm
- using that, provide a simple demo website that has two input fields
- one field takes a JSON payload
- the second a TRL program
- running the program shows the execution result on the page
- TRL runs locally in the browser, no web-server required
