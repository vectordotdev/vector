#!/usr/bin/env python3
"""Send regulated StatsD metric traffic to a TCP listener."""

import argparse
import random
import socket
import string
import time

INCREMENTING_TAG_MODULUS = 1_000_000


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate regulated StatsD traffic over TCP."
    )
    parser.add_argument("--host", default="127.0.0.1", help="StatsD host")
    parser.add_argument("--port", type=int, default=8125, help="StatsD TCP port")
    parser.add_argument(
        "--rate",
        type=int,
        default=2000,
        help="Metric points per second",
    )
    parser.add_argument(
        "--duration",
        type=int,
        default=120,
        help="Duration in seconds",
    )
    parser.add_argument(
        "--metric",
        default="perf.load.counter",
        help="Metric name or prefix when --metric-count > 1",
    )
    parser.add_argument(
        "--metric-count",
        type=int,
        default=1,
        help="Number of metric names to cycle through",
    )
    parser.add_argument(
        "--tags",
        default="env:perf,run:regulated",
        help="Comma-separated StatsD tags",
    )
    parser.add_argument(
        "--cardinality-tag-key",
        default="",
        help="Optional tag key for bounded cardinality values",
    )
    parser.add_argument(
        "--cardinality-tag-count",
        type=int,
        default=0,
        help="Number of values for --cardinality-tag-key (e.g. 100 -> key:0..99)",
    )
    parser.add_argument(
        "--random-tag-key",
        default="rand",
        help="Tag key used for random per-point values",
    )
    parser.add_argument(
        "--random-tag-bytes",
        type=int,
        default=0,
        help="If >0, append random per-point tag value of this size",
    )
    parser.add_argument(
        "--high-cardinality-tags-count",
        type=int,
        default=0,
        help="Number of additional high-cardinality tags per point",
    )
    parser.add_argument(
        "--high-cardinality-tag-prefix",
        default="hc",
        help="Prefix for high-cardinality tag keys",
    )
    parser.add_argument(
        "--high-cardinality-deterministic",
        action="store_true",
        help="Use deterministic values for high-cardinality tags",
    )
    parser.add_argument(
        "--high-cardinality-values-count",
        type=int,
        default=0,
        help=(
            "If >0 with --high-cardinality-deterministic, cycle deterministic values "
            "0..N-1 instead of using an ever-increasing counter"
        ),
    )
    parser.add_argument(
        "--high-cardinality-value-bytes",
        type=int,
        default=0,
        help=(
            "If >0, force each high-cardinality tag value to this exact length. "
            "Deterministic values are zero-padded on the left."
        ),
    )
    parser.add_argument(
        "--incrementing-tag-key",
        default="",
        help="Optional tag key with deterministic per-event incrementing value.",
    )
    parser.add_argument(
        "--incrementing-tag-start",
        type=int,
        default=0,
        help=(
            "Starting value for --incrementing-tag-key (default: 0). "
            f"Values wrap at {INCREMENTING_TAG_MODULUS}."
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    if args.rate <= 0:
        raise ValueError("--rate must be > 0")
    if args.duration <= 0:
        raise ValueError("--duration must be > 0")
    if args.metric_count <= 0:
        raise ValueError("--metric-count must be > 0")
    if args.cardinality_tag_count < 0:
        raise ValueError("--cardinality-tag-count must be >= 0")
    if args.cardinality_tag_count > 0 and not args.cardinality_tag_key:
        raise ValueError("--cardinality-tag-key must be set when --cardinality-tag-count > 0")
    if args.random_tag_bytes < 0:
        raise ValueError("--random-tag-bytes must be >= 0")
    if args.high_cardinality_tags_count < 0:
        raise ValueError("--high-cardinality-tags-count must be >= 0")
    if args.high_cardinality_values_count < 0:
        raise ValueError("--high-cardinality-values-count must be >= 0")
    if args.high_cardinality_value_bytes < 0:
        raise ValueError("--high-cardinality-value-bytes must be >= 0")
    if args.incrementing_tag_start < 0:
        raise ValueError("--incrementing-tag-start must be >= 0")
    if (
        args.high_cardinality_tags_count > 0
        and not args.high_cardinality_deterministic
        and args.random_tag_bytes <= 0
        and args.high_cardinality_value_bytes <= 0
    ):
        raise ValueError(
            "--random-tag-bytes (or --high-cardinality-value-bytes) must be > 0 "
            "when using non-deterministic high-cardinality tags"
        )

    interval = 1.0 / args.rate
    total_to_send = args.rate * args.duration
    alphabet = string.ascii_letters + string.digits
    base_tags = args.tags.strip(",")

    start = time.perf_counter()
    next_tick = start

    with socket.create_connection((args.host, args.port)) as sock:
        for i in range(total_to_send):
            if args.metric_count == 1:
                metric_name = args.metric
                metric_idx = 0
            else:
                metric_idx = i % args.metric_count
                metric_name = f"{args.metric}.{metric_idx}"

            tags = base_tags

            if args.cardinality_tag_count > 0:
                cardinality_tag = (
                    f"{args.cardinality_tag_key}:{i % args.cardinality_tag_count}"
                )
                tags = f"{tags},{cardinality_tag}" if tags else cardinality_tag

            if args.random_tag_bytes > 0:
                random_value = "".join(
                    random.choices(alphabet, k=args.random_tag_bytes)
                )
                random_tag = f"{args.random_tag_key}:{random_value}"
                tags = f"{tags},{random_tag}" if tags else random_tag

            if args.high_cardinality_tags_count > 0:
                for tag_idx in range(args.high_cardinality_tags_count):
                    if args.high_cardinality_deterministic:
                        if args.high_cardinality_values_count > 0:
                            value = (
                                metric_idx + tag_idx
                            ) % args.high_cardinality_values_count
                        else:
                            value = metric_idx + tag_idx
                        hc_value = str(value)
                        if args.high_cardinality_value_bytes > 0:
                            hc_value = hc_value.zfill(args.high_cardinality_value_bytes)[
                                -args.high_cardinality_value_bytes :
                            ]
                    else:
                        value_len = (
                            args.high_cardinality_value_bytes
                            if args.high_cardinality_value_bytes > 0
                            else args.random_tag_bytes
                        )
                        hc_value = "".join(
                            random.choices(alphabet, k=value_len)
                        )

                    hc_tag = f"{args.high_cardinality_tag_prefix}{tag_idx}:{hc_value}"
                    tags = f"{tags},{hc_tag}" if tags else hc_tag

            if args.incrementing_tag_key:
                inc_value = (args.incrementing_tag_start + i) % INCREMENTING_TAG_MODULUS
                inc_tag = f"{args.incrementing_tag_key}:{inc_value}"
                tags = f"{tags},{inc_tag}" if tags else inc_tag

            if tags:
                payload = f"{metric_name}:1|c|#{tags}\n".encode("utf-8")
            else:
                payload = f"{metric_name}:1|c\n".encode("utf-8")

            sock.sendall(payload)
            next_tick += interval
            now = time.perf_counter()
            if next_tick > now:
                time.sleep(next_tick - now)

    elapsed = time.perf_counter() - start
    actual_rate = total_to_send / elapsed if elapsed > 0 else 0.0
    print(
        f"Sent {total_to_send} points in {elapsed:.2f}s "
        f"(target rate={args.rate}/s, actual rate={actual_rate:.1f}/s)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
