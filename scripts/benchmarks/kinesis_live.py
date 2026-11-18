#!/usr/bin/env python3
"""Opt-in live Kinesis acceptance check. Default invocation only prints a plan."""

import argparse
import base64
from collections import Counter
from contextlib import contextmanager
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import time
import uuid

from kpl_decode import decode_kpl


def decode_record(record, fmt, compression):
    data = base64.b64decode(record["Data"], validate=True)
    key = record["PartitionKey"]
    if len(data) + len(key.encode()) > 1048576:
        raise ValueError("physical record exceeds the producer's 1 MiB limit")
    if fmt == "kpl":
        payloads = []
        for inner_key, explicit_hash, payload in decode_kpl(data):
            if inner_key != key or explicit_hash is not None:
                raise ValueError("KPL inner/outer key mismatch or unexpected explicit hash key")
            payloads.append(gzip.decompress(payload) if compression == "gzip" else payload)
    else:
        if data.startswith(b"\xf3\x89\x9a\xc2"):
            raise ValueError("unexpected KPL envelope in newline mode")
        if compression == "gzip":
            data = gzip.decompress(data)
        if not data.endswith(b"\n"):
            raise ValueError("newline payload lacks its final delimiter")
        payloads = data[:-1].split(b"\n")
    return [(payload, key) for payload in payloads]


def validate_records(records, expected, fmt, compression):
    if any(len(decode_record(record, fmt, compression)) > 20 for record in records):
        raise ValueError("physical record exceeds the configured 20 original logs")
    actual = [item for record in records for item in decode_record(record, fmt, compression)]
    want = Counter(payload for payload, _ in expected)
    got = Counter(payload for payload, _ in actual)
    if want != got:
        missing, extra = want - got, got - want
        duplicates = sum(max(0, count - want[payload]) for payload, count in got.items()
                         if payload in want)
        raise ValueError(f"payload mismatch: missing={sum(missing.values())}, "
                         f"extra={sum(extra.values())}, duplicate_originals={duplicates}")
    keys = dict(expected)
    for payload, key in actual:
        if keys[payload] is not None and keys[payload] != key:
            raise ValueError("original payload partition key mismatch")
    return {"original_logs": len(actual), "physical_records": len(records),
            "data_bytes": sum(len(base64.b64decode(record["Data"])) for record in records),
            "max_logs_per_record": max((len(decode_record(r, fmt, compression)) for r in records),
                                       default=0)}


def check_identity(identity, account):
    if identity.get("Account") != account:
        raise ValueError("AWS account does not match --account-id; no stream was created")


class Aws:
    def __init__(self, region, profile):
        self.region, self.profile = region, profile

    def endpoint(self, service):
        suffix = "amazonaws.com.cn" if self.region.startswith("cn-") else "amazonaws.com"
        return f"https://{service}.{self.region}.{suffix}"

    def call(self, service, action, *args):
        command = ["aws", "--region", self.region, "--output", "json", "--no-cli-pager",
                   "--no-cli-auto-prompt", "--cli-connect-timeout", "5", "--cli-read-timeout", "20"]
        if self.profile:
            command += ["--profile", self.profile]
        if service in ("sts", "kinesis"):
            command += ["--endpoint-url", self.endpoint(service)]
        command += [service, action, *args]
        result = subprocess.run(command, text=True, capture_output=True,
                                timeout=180 if action == "wait" else 60)
        if result.returncode:
            raise RuntimeError(f"AWS {service} {action} failed: {result.stderr.strip()}")
        return json.loads(result.stdout) if result.stdout.strip() else {}


@contextmanager
def temporary_stream(aws, name, report):
    report["stream_name"] = name
    report["cleanup"] = "not_created"
    report["creation_attempted"] = True
    aws.call("kinesis", "create-stream", "--stream-name", name, "--shard-count", "1",
             "--stream-mode-details", "StreamMode=PROVISIONED")
    report["cleanup"] = "pending"
    try:
        yield
    finally:
        failing = sys.exc_info()[0] is not None
        try:
            aws.call("kinesis", "delete-stream", "--stream-name", name)
            aws.call("kinesis", "wait", "stream-not-exists", "--stream-name", name)
            report["cleanup"] = "deleted"
        except BaseException as error:
            report["cleanup"] = "failed"
            report["cleanup_error"] = str(error)
            if not failing:
                raise


def configuration(port, stream, aws, fmt, compression, keys, data_dir):
    return f'''data_dir = {json.dumps(str(data_dir))}
[sources.logs]
type = "socket"
mode = "tcp"
address = "127.0.0.1:{port}"
decoding.codec = "json"
framing.method = "newline_delimited"
[transforms.original]
type = "remap"
inputs = ["logs"]
source = '. = {{"message": string!(.payload), "key": .key}}'
[sinks.kinesis]
type = "aws_kinesis_streams"
inputs = ["original"]
stream_name = {json.dumps(stream)}
region = {json.dumps(aws.region)}
endpoint = {json.dumps(aws.endpoint("kinesis"))}
encoding.codec = "text"
compression = {json.dumps(compression)}
request_retry_partial = true
request.concurrency = 1
healthcheck.enabled = false
acknowledgements.enabled = true
batch.max_events = 500
batch.timeout_secs = 0.05
{('partition_key_field = "key"' if keys != 'random' else '')}
[sinks.kinesis.aggregation]
enabled = true
format = {json.dumps(fmt)}
max_events = 20
max_bytes = 943718
timeout_secs = 1.0
'''


def connect_source(process, port, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Vector exited before opening the source; inspect vector.log")
        try:
            return socket.create_connection(("127.0.0.1", port), timeout=1)
        except OSError:
            time.sleep(0.1)
    raise TimeoutError("Vector source startup timed out")


def run_case(args, aws, stream, iterator, case_dir, fmt, compression, keys, env):
    case_dir.mkdir()
    expected, wire = [], bytearray()
    for index in range(61):
        payload = json.dumps({"case": case_dir.name, "id": index,
                              "text": "unicode λ, escaped LF\n and quote \"", "padding": "x" * 900},
                             ensure_ascii=False, separators=(",", ":"))
        key = "live-κλειδί" if keys == "same" else f"live-{index % 2}"
        expected.append((payload.encode(), None if keys == "random" else key))
        wire.extend(json.dumps({"payload": payload, "key": key}, ensure_ascii=False).encode() + b"\n")
    (case_dir / "expected.json").write_text(json.dumps([
        {"payload_sha256": hashlib.sha256(payload).hexdigest(), "key": key}
        for payload, key in expected], indent=2) + "\n")
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    config = configuration(port, stream, aws, fmt, compression, keys, case_dir)
    (case_dir / "vector.toml").write_text(config)
    records, sequences, rereads = [], set(), 0
    received = 0
    started = time.monotonic()
    with (case_dir / "vector.log").open("w") as log:
        process = subprocess.Popen([str(args.binary), "--config", str(case_dir / "vector.toml"),
                                    "--threads", "2"], env=env, stdout=log, stderr=subprocess.STDOUT)
        sock = None
        try:
            sock = connect_source(process, port, min(30, args.timeout))
            sock.settimeout(10)
            sock.sendall(wire)
            deadline = time.monotonic() + args.timeout
            while received < len(expected):
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"received {received}/{len(expected)} original logs")
                if process.poll() is not None:
                    raise RuntimeError("Vector exited while waiting for delivery")
                page = aws.call("kinesis", "get-records", "--shard-iterator", iterator, "--limit", "1000")
                iterator = page["NextShardIterator"]
                for record in page["Records"]:
                    sequence = record["SequenceNumber"]
                    if sequence in sequences:
                        rereads += 1
                        continue
                    sequences.add(sequence)
                    records.append(record)
                    received += len(decode_record(record, fmt, compression))
                time.sleep(0.25)
            sock.close()
            sock = None
            process.send_signal(signal.SIGTERM)
            process.wait(timeout=30)
            if process.returncode:
                raise RuntimeError(f"Vector exited with {process.returncode}; inspect vector.log")
            quiet_until = time.monotonic() + 2
            while time.monotonic() < quiet_until:
                page = aws.call("kinesis", "get-records", "--shard-iterator", iterator, "--limit", "1000")
                iterator = page["NextShardIterator"]
                for record in page["Records"]:
                    if record["SequenceNumber"] in sequences:
                        rereads += 1
                    else:
                        sequences.add(record["SequenceNumber"])
                        records.append(record)
                time.sleep(0.25)
            result = validate_records(records, expected, fmt, compression)
            if keys != "alternating" and result["max_logs_per_record"] < 2:
                raise ValueError("delivery passed but aggregation did not occur")
            if keys == "alternating" and result["physical_records"] != len(expected):
                raise ValueError("alternating keys must not share an aggregate")
            result.update(format=fmt, compression=compression, keys=keys, reread_records=rereads,
                          elapsed_seconds=time.monotonic() - started,
                          config_sha256=hashlib.sha256(config.encode()).hexdigest())
            (case_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            return iterator, result
        finally:
            try:
                if sock is not None:
                    sock.close()
            finally:
                try:
                    if process.poll() is None:
                        process.terminate()
                        try:
                            process.wait(timeout=30)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                finally:
                    (case_dir / "records.json").write_text(json.dumps(records, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--region", required=True)
    parser.add_argument("--account-id", required=True)
    parser.add_argument("--profile")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--execute", action="store_true", help="create, use and delete a billable stream")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9]{12}", args.account_id):
        parser.error("--account-id must have 12 digits")
    if not re.fullmatch(r"[a-z0-9-]+", args.region):
        parser.error("invalid region")
    if not 5 <= args.timeout <= 600:
        parser.error("--timeout must be between 5 and 600 seconds")
    cases = [(fmt, compression, keys) for fmt in ("newline", "kpl")
             for compression, keys in [("none", "random"), ("none", "same"),
                                       ("none", "alternating"), ("gzip", "same")]]
    plan = {"execute": args.execute, "account_id": args.account_id, "region": args.region,
            "profile": args.profile, "cases": cases, "original_logs_per_case": 61,
            "resources": "one unique PROVISIONED shard, deleted in finally",
            "acceptance_only": True}
    if not args.execute:
        print(json.dumps(plan, indent=2))
        return 0
    args.binary = args.binary.resolve(strict=True)
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    aws = Aws(args.region, args.profile)
    report = {"plan": plan, "cases": [], "passed": False, "cleanup": "not_created",
              "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}
    try:
        identity = aws.call("sts", "get-caller-identity")
        check_identity(identity, args.account_id)
        report["identity"] = identity
        report["binary_version"] = subprocess.check_output([str(args.binary), "--version"],
                                                           text=True, timeout=10).strip()
        credentials = aws.call("configure", "export-credentials", "--format", "process")
        env = os.environ.copy()
        for name in ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN", "AWS_SECURITY_TOKEN"):
            env.pop(name, None)
        for source, target in [("AccessKeyId", "AWS_ACCESS_KEY_ID"),
                               ("SecretAccessKey", "AWS_SECRET_ACCESS_KEY"),
                               ("SessionToken", "AWS_SESSION_TOKEN")]:
            if credentials.get(source):
                env[target] = credentials[source]
        if not env.get("AWS_ACCESS_KEY_ID") or not env.get("AWS_SECRET_ACCESS_KEY"):
            raise ValueError("AWS CLI did not export usable credentials")
        env["AWS_REGION"] = args.region
        stream = "vector-aggregation-live-" + uuid.uuid4().hex
        report.update(stream_name=stream, creation_attempted=True)
        (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"Temporary stream: {stream}", flush=True)
        with temporary_stream(aws, stream, report):
            aws.call("kinesis", "wait", "stream-exists", "--stream-name", stream)
            summary = aws.call("kinesis", "describe-stream-summary", "--stream-name", stream)["StreamDescriptionSummary"]
            report["stream_summary"] = summary
            if summary["StreamStatus"] != "ACTIVE" or summary["OpenShardCount"] != 1:
                raise ValueError("expected one ACTIVE shard")
            if "RecordDistributionStrategy" in summary and summary["RecordDistributionStrategy"] != "USER_PARTITION_KEY":
                raise ValueError("stream has an incompatible distribution strategy")
            shards = aws.call("kinesis", "list-shards", "--stream-name", stream)["Shards"]
            if len(shards) != 1 or "EndingSequenceNumber" in shards[0]["SequenceNumberRange"]:
                raise ValueError("expected exactly one open shard")
            iterator = aws.call("kinesis", "get-shard-iterator", "--stream-name", stream,
                                "--shard-id", shards[0]["ShardId"], "--shard-iterator-type", "TRIM_HORIZON")["ShardIterator"]
            for fmt, compression, keys in cases:
                case_dir = args.out / f"{fmt}-{compression}-{keys}"
                print(f"Testing {case_dir.name}", flush=True)
                iterator, result = run_case(args, aws, stream, iterator, case_dir,
                                             fmt, compression, keys, env)
                report["cases"].append(result)
            report["passed"] = True
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
        report["passed"] = False
    finally:
        (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report.get(key) for key in ("passed", "cleanup", "error", "stream_name")}, indent=2))
    return 0 if report["passed"] and report["cleanup"] == "deleted" else 1


if __name__ == "__main__":
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt("termination requested; attempting cleanup")

    signal.signal(signal.SIGTERM, interrupted)
    raise SystemExit(main())
