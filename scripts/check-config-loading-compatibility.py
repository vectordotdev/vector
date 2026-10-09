#!/usr/bin/env python3
"""Compare config acceptance and finite pipeline output across Vector binaries."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time


CORPUS = Path(__file__).resolve().parents[1] / "tests/config-loading-compatibility"


def binary_argument(value):
    label, separator, filename = value.partition("=")
    if not separator or not re.fullmatch(r"[A-Za-z0-9_-]+", label):
        raise argparse.ArgumentTypeError("expected LABEL=/absolute/path/to/vector")
    path = Path(filename).expanduser().resolve()
    if not path.is_file() or not os.access(path, os.X_OK):
        raise argparse.ArgumentTypeError(f"not an executable file: {path}")
    return label, path


def execute(command, cwd, environment, timeout):
    started = time.monotonic()
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=environment,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        return {
            "command": command,
            "status": exit_status(result.returncode),
            "exit_code": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "seconds": round(time.monotonic() - started, 3),
        }
    except subprocess.TimeoutExpired as error:
        return {
            "command": command,
            "status": "timeout",
            "exit_code": None,
            "stdout": decode_partial(error.stdout),
            "stderr": decode_partial(error.stderr),
            "seconds": round(time.monotonic() - started, 3),
        }


def decode_partial(value):
    return value.decode(errors="replace") if isinstance(value, bytes) else value or ""


def exit_status(code):
    if code < 0:
        return "crashed"
    return "accepted" if code == 0 else "rejected"


def prepare_case(case, workdir):
    workdir.mkdir(parents=True)
    data_dir = workdir / "data"
    data_dir.mkdir()
    secrets_path = workdir / "mock-secrets.json"
    secrets_path.write_text(json.dumps(case.get("secrets", {})) + "\n")
    replacements = {
        "DATA_DIR": str(data_dir),
        "SECRETS_FILE": str(secrets_path),
        "COUNT": "2",
        "INTERVAL": "0.01",
        "SEQUENCE": "false",
        "VALUE": "'fixture-message'",
        "HEALTHCHECK": "{}",
        "FRAMING": "method: newline_delimited",
        "SECRET_CONFIG": "",
        **case.get("replace", {}),
    }
    # Corpus replacements may refer to generated paths, but never interpolate env vars.
    for key, value in tuple(replacements.items()):
        replacements[key] = value.replace("@SECRETS_FILE@", str(secrets_path))
    for destination, fixture in case["files"].items():
        target = workdir / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        text = (CORPUS / "fixtures" / f"{fixture}.in").read_text()
        for key, value in replacements.items():
            text = text.replace(f"@{key}@", value)
        unresolved = re.findall(r"@[A-Z_]+@", text)
        if unresolved:
            raise ValueError(f"{case['name']}: unresolved fixture tokens: {unresolved}")
        target.write_text(text)


def run_case(binary, case, workdir, environment, timeout):
    prepare_case(case, workdir)
    command = [str(binary), "--color", "never"]
    mode = case.get("mode", "validate")
    if mode == "validate":
        command.extend(["validate", "--no-environment"])
    elif mode == "test":
        command.append("test")
    elif mode != "run":
        raise ValueError(f"unknown mode: {mode}")
    if case.get("interpolate_env", False):
        command.append("--dangerously-allow-env-var-interpolation")
    if "directory" in case:
        command.extend(["--config-dir", str(workdir / case["directory"])])
    else:
        paths = case.get("paths", ["vector.yaml"])
        for path in paths:
            if mode == "run":
                command.append("--config")
            command.append(str(workdir / path))
    result = execute(command, workdir, environment, timeout)
    if mode == "run" and result["status"] == "accepted":
        try:
            result["events"] = [
                json.loads(line) for line in result["stdout"].splitlines() if line.strip()
            ]
        except json.JSONDecodeError as error:
            result["status"] = "invalid-output"
            result["output_error"] = str(error)
    if mode == "test":
        result["passed_tests"] = re.findall(
            r"^test (.+) \.\.\. passed$", result["stdout"], flags=re.MULTILINE
        )
    (workdir / "stdout.log").write_text(result["stdout"])
    (workdir / "stderr.log").write_text(result["stderr"])
    return result


def outcome(result):
    # Error text is retained for diagnosis, but not a compatibility requirement.
    return (
        result["status"],
        result["exit_code"],
        result.get("events"),
        result.get("passed_tests"),
    )


def expectation_issues(case, result, parse_first=False):
    prefix = "parse_first" if parse_first and case.get("breaking") else "released"
    status = case[f"{prefix}_status"]
    issues = []
    if result["status"] != status:
        issues.append(f"expected {status}, got {result['status']}")
    expected_exit = 0 if status == "accepted" else 78
    if result["exit_code"] != expected_exit:
        issues.append(f"expected exit {expected_exit}, got {result['exit_code']}")
    messages = case.get(f"{prefix}_messages")
    if messages is not None and result.get("events") != [
        {"message": message} for message in messages
    ]:
        issues.append("events differ from the explicit fixture expectation")
    if "passed_tests" in case and result.get("passed_tests") != case["passed_tests"]:
        issues.append("passed test names/count differ from the fixture expectation")
    return issues


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=binary_argument, action="append", required=True)
    parser.add_argument("--reference", help="reference label (defaults to the first binary)")
    parser.add_argument("--case", action="append", help="run cases containing this substring")
    parser.add_argument(
        "--output", type=Path, help="new directory for configs, logs and results"
    )
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    binaries = dict(args.binary)
    if len(binaries) != len(args.binary):
        parser.error("binary labels must be unique")
    reference = args.reference or args.binary[0][0]
    if reference not in binaries:
        parser.error("--reference must name one of the supplied binaries")
    cases = json.loads((CORPUS / "cases.json").read_text())
    if args.case:
        cases = [case for case in cases if any(name in case["name"] for name in args.case)]
    if not cases:
        parser.error("no cases matched")
    output = (
        args.output.resolve()
        if args.output
        else Path(tempfile.mkdtemp(prefix="vector-config-compat-"))
    )
    if args.output:
        output.mkdir(parents=True, exist_ok=False)
    # Do not inherit Vector options/config paths or accidental fixture variables.
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("VECTOR_", "COMPAT_"))
    }
    environment.update({"VECTOR_LOG": "error", "NO_COLOR": "1"})
    report = {"reference": reference, "binaries": {}, "cases": []}
    for label, binary in binaries.items():
        version = execute([str(binary), "--version"], output, environment, args.timeout)
        if version["status"] != "accepted":
            parser.error(f"{label}: cannot execute binary: {version['stderr']}")
        with binary.open("rb") as binary_file:
            digest = hashlib.file_digest(binary_file, "sha256").hexdigest()
        report["binaries"][label] = {
            "path": str(binary),
            "sha256": digest,
            "version": version["stdout"].strip(),
        }
    failures = 0
    for case in cases:
        entry = {"name": case["name"], "breaking": case.get("breaking"), "runs": {}}
        for label, binary in binaries.items():
            run_environment = {**environment, **case.get("env", {})}
            entry["runs"][label] = run_case(
                binary, case, output / label / case["name"], run_environment, args.timeout
            )
        baseline = entry["runs"][reference]
        issues = [
            f"{label}: {issue}"
            for label, result in entry["runs"].items()
            for issue in expectation_issues(case, result, parse_first=label != reference)
        ]
        differences = [
            label
            for label, result in entry["runs"].items()
            if label != reference and outcome(result) != outcome(baseline)
        ]
        for label, result in entry["runs"].items():
            if result["status"] in ("timeout", "invalid-output", "crashed"):
                issues.append(f"{label}: {result['status']}")
        if differences and not case.get("breaking"):
            issues.append("unexpected differences: " + ", ".join(differences))
        entry.update({"differences": differences, "issues": issues})
        report["cases"].append(entry)
        failures += bool(issues)
        status = "FAIL" if issues else "EXPECTED-DIFFERENCE" if differences else "PASS"
        states = " ".join(
            f"{label}={result['status']}" for label, result in entry["runs"].items()
        )
        print(f"{status:19} {case['name']}: {states}", flush=True)
        for issue in issues:
            print(f"  {issue}", flush=True)
        (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"\n{len(cases)} cases; {failures} failures. Report: {output / 'results.json'}")
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())
