# Live Kinesis aggregation acceptance

This runner prepares or executes a small functional test against real AWS
Kinesis. It uses the Vector binary as the producer, AWS CLI v2 as the reader,
and an independent Python KPL decoder. It does not require Python packages.

**The default invocation only prints a plan.** It does not use AWS credentials,
contact AWS, start Vector, or create an output directory. Add `--execute` to
create a billable, provisioned, single-shard stream and run the checks.

## Prepare

Build the branch with its pinned Rust toolchain:

```sh
cargo build --release --locked --no-default-features \
  --features sinks-aws_kinesis_streams,sources-socket,transforms-remap --bin vector
python3 -m unittest discover -s scripts/benchmarks -p test_kinesis_live.py
```

Select an isolated AWS test account and an AWS CLI profile with current
credentials. For an SSO profile, refresh it with `aws sso login --profile sandbox`.
Replace the example account ID and profile below with the test account values.
Use a new output directory for each execution.

```sh
python3 scripts/benchmarks/kinesis_live.py \
  --binary target/release/vector \
  --account-id 123456789012 --region us-east-1 --profile sandbox \
  --out /private/tmp/vector-kinesis-live-run-001
```

Read the plan, then run the same command with `--execute` to start the live test.
The runner checks STS account identity before stream creation. The CLI and Vector
use the same resolved credentials; the runner exports credentials directly to
the child process environment and does not write them into evidence files.
Endpoints are fixed to the requested region's AWS service endpoints.

## Permissions and resources

The test creates one stream named `vector-aggregation-live-<random UUID>`, using
PROVISIONED mode with one shard. It does not require stream-tagging permission.
Restrict the test principal's Kinesis permissions to
`arn:aws:kinesis:REGION:ACCOUNT_ID:stream/vector-aggregation-live-*`:

- `kinesis:CreateStream`
- `kinesis:DescribeStream` and `kinesis:DescribeStreamSummary`
- `kinesis:ListShards`, `kinesis:GetShardIterator`, and `kinesis:GetRecords`
- `kinesis:PutRecords` and `kinesis:DeleteStream`

STS caller identity must also be available. KPL requires a stream with an omitted
legacy distribution field or `USER_PARTITION_KEY` distribution. The runner checks
the returned summary and requires exactly one open shard before sending data.
Each KPL Vector process also performs its mandatory startup summary check with
optional healthchecks disabled.

## Acceptance and evidence

The eight cases cover newline and KPL, each with random keys, a constant explicit
key, alternating explicit keys, and gzip with a constant key. Each sends 61
unique compact JSON payloads with escaped LF, quotes, and Unicode. Aggregates
hold at most 20 logs, with a one-second aggregation timer and 50 ms API batching.
The source stays open during receipt, so the underfilled tail requires a timer.

The reader starts at `TRIM_HORIZON` and follows shard iterators across empty
pages, polling at most four times per second. It retains the iterator across
cases, waits for Vector to exit cleanly, and reads for another two seconds.
Every physical record is checked against the conservative 1 MiB limit including
its outer key. The independent reader checks KPL magic, protobuf boundaries,
MD5, and inner/outer key parity. Gzip is decoded per KPL user record or once per
newline aggregate. Exact payload bytes and explicit keys must match the fixture.
Random keys must agree inside and outside KPL. Matching keys must aggregate;
alternating keys must remain separate.

The output retains the binary SHA-256 and version, STS identity, stream summary,
each generated configuration and its SHA-256, fixture hashes, raw GetRecords
records, Vector logs, and per-case results. The exit code is zero only when all
cases pass and stream deletion is confirmed. Keep the output private when it
contains account IDs, ARNs, or local paths. An optional `--timeout` bounds each
case's receipt wait; AWS calls and Vector shutdown have separate bounds.

This is at-least-once delivery: repeated reads of one physical sequence number
are counted separately from duplicate original payloads in new physical records.
Missing, extra, corrupt, or duplicate payloads fail this controlled test. It does
not establish an exactly-once service guarantee. Two quiet seconds do not prove
the absence of arbitrarily late duplicates.

The result is a live Kinesis smoke test. It does not establish sustainable
throughput, exercise an actual KCL consumer, test Firehose JSON deaggregation, or
force AWS partial failures/throttling. Those are separate acceptance tasks.
Use uncompressed JSON for Firehose built-in JSON processing. Keep local injected
partial-retry results separate from claims about live AWS retries.

## Cleanup

After a successful create response, the runner owns that stream and deletes it
in `finally`, including after validation failure or an interrupt. It waits for
the stream to disappear and records the cleanup result in `report.json`. It
never adopts an existing stream and does not force-delete registered consumers.

If creation fails with an ambiguous network error, AWS might have created the
stream even though the runner did not receive confirmation. A hard kill, power
loss, expired credentials, or denied deletion can also leave a stream behind.
Inspect the recorded name and AWS account, then delete only that test stream:

```sh
aws --profile sandbox --region us-east-1 kinesis describe-stream-summary \
  --stream-name vector-aggregation-live-REPLACE_WITH_RECORDED_UUID
aws --profile sandbox --region us-east-1 kinesis delete-stream \
  --stream-name vector-aggregation-live-REPLACE_WITH_RECORDED_UUID
aws --profile sandbox --region us-east-1 kinesis wait stream-not-exists \
  --stream-name vector-aggregation-live-REPLACE_WITH_RECORDED_UUID
```
