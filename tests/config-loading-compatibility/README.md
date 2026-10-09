# Configuration-loading compatibility corpus

This black-box corpus compares released Vector with one or more local binaries. It
does not build Vector, install a binary, access external services, or use real
secrets. Cases exercise finite `demo_logs -> remap -> console` pipelines, config
validation, and namespaced configuration unit tests.

Run from the repository root with Python 3.11 or newer:

```shell
python3 scripts/check-config-loading-compatibility.py \
  --binary release=/absolute/path/to/released/vector \
  --binary before=/absolute/path/to/pre-refactor/vector \
  --binary after=/absolute/path/to/refactored/vector
```

The first binary is the released reference by default. `--reference LABEL` selects
another one; release expectations in the manifest still apply to it. All other
binaries are checked against the parse-first contract, including explicit expected
results for documented breaking changes. `--case SUBSTRING` restricts the cases and
can be repeated. `--timeout SECONDS` sets each invocation's timeout (default: 30).
`--output /new/directory` sets the report location; otherwise a fresh temporary
directory is created. Existing explicit output directories are refused.

Each invocation has isolated configs, data directories, and mock secret files.
The runner removes inherited `VECTOR_*` and `COMPAT_*` environment variables,
then sets only the fixture-specific options. It records binary versions and
SHA-256 digests, complete commands, exit codes, stdout/stderr, elapsed time, and
parsed output events in `results.json`. All generated files and separate logs
remain in the report directory for inspection.

`cases.json` explicitly states the released binary's expected acceptance and,
for successful finite runtime cases, complete output events after a remap removes
timestamps and other nondeterministic source metadata. Directory cases use equal
logical values where file enumeration order is unspecified. An invalid merge and
duplicate component case check rejection independently.

Only cases with a `breaking` explanation may differ from the release, and their
`parse_first_status` and optional `parse_first_messages` specify the exact expected
new outcome. Those reasons correspond to
`changelog.d/parse_first_interpolation.breaking.md`: no interpolation of
keys/comments, no structural expansion, quote raw TOML/JSON placeholders, and
remove outer-format escaping. Every case checks its expected exit code (0 for
success, 78 for invalid configuration). The namespaced test case also requires its
exact passing test name and count, so silently loading zero tests cannot pass.
Error text is retained for inspection but is not compared verbatim. Timeouts,
crashes, and malformed runtime output always fail.

Run the runner's own false-positive regression tests without a Vector binary:

```shell
python3 tests/config-loading-compatibility/test_runner.py
```

The `.yaml.in`, `.toml.in`, and `.json.in` fixture templates use `@NAME@` tokens for
generated paths and manifest-defined variants. They are not interpolation syntax.
Environment placeholders and `SECRET[...]` references remain untouched until
Vector loads the generated configuration.

The initially verified macOS arm64 baseline was the official `v0.58.0` asset:

- [Release archive](https://github.com/vectordotdev/vector/releases/download/v0.58.0/vector-0.58.0-arm64-apple-darwin.tar.gz)
- GitHub release asset SHA-256: `9182491597f1bdedb08d84a051616c62deea770a9d905b697712cc6526919449`

Download/extract baselines only into a fresh temporary directory, compare the
archive checksum against release metadata before execution, and never install or
replace a system binary. The runner deliberately does not download executables.
