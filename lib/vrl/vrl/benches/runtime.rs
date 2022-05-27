use std::{collections::HashMap, time::Duration};

use ::value::Value;
use compiler::{state, Resolved};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use indoc::indoc;
use vector_common::TimeZone;
use vrl::Runtime;

struct Source {
    name: &'static str,
    target: &'static str,
    program: &'static str,
}

use vrl_stdlib::{uuid_v4, vrl_fn_downcase as downcase, vrl_fn_upcase as upcase};

#[inline(never)]
#[no_mangle]
pub extern "C" fn vrl_fn_uuid_v4(resolved: &mut Resolved) {
    println!("yo? uuid?");
    *resolved = uuid_v4()
}

extern "C" {
    fn vrl_fn_upcase(value: *mut Resolved, resolved: *mut Resolved);
}

static SOURCES: &[Source] = &[
    Source {
        name: "starts_with",
        target: "{}",
        program: indoc! {r#"
            status = string(.foo) ?? ""
            .status = starts_with("a", status, true)
        "#},
    },
    Source {
        name: "parse_groks_bla",
        target: "{}",
        program: indoc! {r#"
            .custom.message = "127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] \"GET /apache_pb.gif HTTP/1.0\" 200 2326"
            parse_groks!(value: .custom.message,
                patterns: [
                    "(?s)%{_prefix} %{regex(\"Compacting\"):db.operation}.* %{_keyspace}\\/%{_table}:%{data:partition_key} \\(%{_bytes} bytes\\)",
                    "(?s)%{_prefix} %{regex(\"Flushing\"):db.operation}.*\\(Keyspace='%{_keyspace}', ColumnFamily='%{_table}'\\) %{data}: %{_onheap_total}\\/%{_offheap_total}, live: %{_onheap_live}\\/%{_offheap_live}, flushing: %{_onheap_flush}\\/%{_offheap_flush}, this: %{_onheap_this}\\/%{_offheap_this}",
                    "(?s)%{_prefix} %{regex(\"Enqueuing\"):db.operation}.* of %{_keyspace}: %{_onheap_bytes}%{data} \\(%{_onheap_pct}%\\) on-heap, %{_offheap_bytes} \\(%{_offheap_pct}%\\).*",
                    "(?s)%{_prefix} %{regex(\"Writing\"):db.operation}.*-%{_keyspace}%{data}\\(%{number:cassandra.bytes:scale(1000000)}%{data}, %{integer:cassandra.ops} ops, %{_onheap_pct}%\\/%{_offheap_pct}.*",
                    "(?s)%{_prefix} Completed %{regex(\"flushing\"):db.operation} %{_sstable} \\(%{number:cassandra.bytes_kb}KiB\\) for commitlog %{data:commitlog}",
                    "(?s)%{_prefix}\\s+%{regex(\"Compacted\"):db.operation}.* to \\[%{_sstable}\\].\\s+%{notSpace:cassandra.bytes_in} bytes to %{notSpace:cassandra.bytes_out} \\(\\~%{integer:cassandra.percent_of_orig}% of original\\) in %{notSpace:cassandra.duration_ms}ms = %{number:cassandra.speed_mb}MB/s.\\s+%{notSpace:cassandra.pkeys_in} total partitions merged to %{notSpace:cassandra.pkeys_out}\\.\\s+Partition merge counts were %{data:cassandra.merge_cnt}",
                    "(?s)%{_prefix} G.* %{integer:duration:scale(1000000)}ms. %{data}: %{integer:cassandra.eden.orig_bytes} -> %{integer:cassandra.eden.new_bytes}; %{data}: %{integer:cassandra.oldgen.orig_bytes} -> %{integer:cassandra.oldgen.new_bytes};.*",
                    "(?s)%{_prefix} %{word:cassandra.pool}\\s*(?>%{integer:cassandra.cache_used}\\s*%{integer:cassandra.cache_size}\\s*all|%{integer:cassandra.threads.active}\\s*%{integer:cassandra.threads.pending}\\s*%{integer:cassandra.threads.completed}\\s*%{integer:cassandra.threads.blocked}\\s*%{integer:cassandra.threads.all_time_blocked}|%{integer:cassandra.threads.active}\\s*%{integer:cassanadra.threads.pending})",
                    "(?s)%{_prefix} %{integer:db.operations} operations were slow in the last %{integer:elapsed_time:scale(1000000)} msecs:\\n%{data:db.slow_statements:array(\"\", \"\\\\n\")}",
                    "(?s)%{_prefix} %{data:msg}",
                ],
                aliases: {
                    "cassandra_compaction_key": "%{_prefix} %{regex(\"Compacting\"):db.operation}.* %{_keyspace}\\/%{_table}:%{data:partition_key} \\(%{_bytes} bytes\\)",
                    "cassandra_pool_cleaner": "%{_prefix} %{regex(\"Flushing\"):db.operation}.*\\(Keyspace='%{_keyspace}', ColumnFamily='%{_table}'\\) %{data}: %{_onheap_total}\\/%{_offheap_total}, live: %{_onheap_live}\\/%{_offheap_live}, flushing: %{_onheap_flush}\\/%{_offheap_flush}, this: %{_onheap_this}\\/%{_offheap_this}",
                    "cassandra_pool_cleaner2": "%{_prefix} %{regex(\"Enqueuing\"):db.operation}.* of %{_keyspace}: %{_onheap_bytes}%{data} \\(%{_onheap_pct}%\\) on-heap, %{_offheap_bytes} \\(%{_offheap_pct}%\\).*",
                    "cassandra_table_flush": "%{_prefix} %{regex(\"Writing\"):db.operation}.*-%{_keyspace}%{data}\\(%{number:cassandra.bytes:scale(1000000)}%{data}, %{integer:cassandra.ops} ops, %{_onheap_pct}%\\/%{_offheap_pct}.*",
                    "cassandra_mem_flush": "%{_prefix} Completed %{regex(\"flushing\"):db.operation} %{_sstable} \\(%{number:cassandra.bytes_kb}KiB\\) for commitlog %{data:commitlog}",
                    "cassandra_compaction": "%{_prefix}\\s+%{regex(\"Compacted\"):db.operation}.* to \\[%{_sstable}\\].\\s+%{notSpace:cassandra.bytes_in} bytes to %{notSpace:cassandra.bytes_out} \\(\\~%{integer:cassandra.percent_of_orig}% of original\\) in %{notSpace:cassandra.duration_ms}ms = %{number:cassandra.speed_mb}MB/s.\\s+%{notSpace:cassandra.pkeys_in} total partitions merged to %{notSpace:cassandra.pkeys_out}\\.\\s+Partition merge counts were %{data:cassandra.merge_cnt}",
                    "cassandra_gc_format": "%{_prefix} G.* %{integer:duration:scale(1000000)}ms. %{data}: %{integer:cassandra.eden.orig_bytes} -> %{integer:cassandra.eden.new_bytes}; %{data}: %{integer:cassandra.oldgen.orig_bytes} -> %{integer:cassandra.oldgen.new_bytes};.*",
                    "cassandra_thread_pending": "%{_prefix} %{word:cassandra.pool}\\s*(?>%{integer:cassandra.cache_used}\\s*%{integer:cassandra.cache_size}\\s*all|%{integer:cassandra.threads.active}\\s*%{integer:cassandra.threads.pending}\\s*%{integer:cassandra.threads.completed}\\s*%{integer:cassandra.threads.blocked}\\s*%{integer:cassandra.threads.all_time_blocked}|%{integer:cassandra.threads.active}\\s*%{integer:cassanadra.threads.pending})",
                    "cassandra_slow_statements": "%{_prefix} %{integer:db.operations} operations were slow in the last %{integer:elapsed_time:scale(1000000)} msecs:\\n%{data:db.slow_statements:array(\"\", \"\\\\n\")}",
                    "cassandra_fallback_parser": "%{_prefix} %{data:msg}",
                    "_level": "%{word:db.severity}",
                    "_thread_name": "%{notSpace:logger.thread_name}",
                    "_thread_id": "%{integer:logger.thread_id}",
                    "_logger_name": "%{notSpace:logger.name}",
                    "_table": "%{word:db.table}",
                    "_sstable": "%{notSpace:cassandra.sstable}",
                    "_bytes": "%{integer:cassandra.bytes}",
                    "_keyspace": "%{word:cassandra.keyspace}",
                    "_onheap_total": "%{number:cassandra.onheap.total}",
                    "_onheap_live": "%{number:cassandra.onheap.live}",
                    "_onheap_flush": "%{number:cassandra.onheap.flush}",
                    "_onheap_this": "%{number:cassandra.onheap.this}",
                    "_onheap_bytes": "%{integer:cassandra.onheap.bytes}",
                    "_onheap_pct": "%{integer:cassandra.onheap.percent}",
                    "_offheap_total": "%{number:cassandra.offheap.total}",
                    "_offheap_live": "%{number:cassandra.offheap.live}",
                    "_offheap_flush": "%{number:cassandra.offheap.flush}",
                    "_offheap_this": "%{number:cassandra.offheap.this}",
                    "_offheap_bytes": "%{integer:cassandra.offheap.bytes}",
                    "_offheap_pct": "%{integer:cassandra.offheap.percent}",
                    "_default_prefix": "%{_level}\\s+\\[(%{_thread_name}:%{_thread_id}|%{_thread_name})\\]\\s+%{date(\"yyyy-MM-dd HH:mm:ss,SSS\"):db.date}\\s+%{word:filename}.java:%{integer:lineno} -",
                    "_suggested_prefix": "%{date(\"yyyy-MM-dd HH:mm:ss\"):db.date} \\[(%{_thread_name}:%{_thread_id}|%{_thread_name})\\] %{_level} %{_logger_name}\\s+-",
                    "_prefix": "(?>%{_default_prefix}|%{_suggested_prefix})"
                }
            )
        "#},
    },
    Source {
        name: "if_false",
        target: "{}",
        program: indoc! {r#"
            if (.foo != null) {
                .derp = 123
            }
        "#},
    },
    Source {
        name: "merge",
        target: "{}",
        program: indoc! {r#"
            merge({ "a": 1, "b": 2 }, { "b": 3, "c": 4 })
        "#},
    },
    Source {
        name: "parse_groks",
        target: "{}",
        program: indoc! {r#"
            parse_groks!(
                "2020-10-02T23:22:12.223222Z info hello world",
                patterns: [
                    "%{common_prefix} %{_status} %{_message}",
                    "%{common_prefix} %{_message}"
                ],
                aliases: {
                    "common_prefix": "%{_timestamp} %{_loglevel}",
                    "_timestamp": "%{TIMESTAMP_ISO8601:timestamp}",
                    "_loglevel": "%{LOGLEVEL:level}",
                    "_status": "%{POSINT:status}",
                    "_message": "%{GREEDYDATA:message}"
                }
            )
        "#},
    },
    Source {
        name: "pipelines_grok",
        target: "{}",
        program: indoc! {r#"
            custom, err = parse_groks(value: .custom.message,
                patterns: [
                    "(?s)%{_prefix} %{regex(\"Compacting\"):db.operation}.* %{_keyspace}\\/%{_table}:%{data:partition_key} \\(%{_bytes} bytes\\)",
                    "(?s)%{_prefix} %{regex(\"Flushing\"):db.operation}.*\\(Keyspace='%{_keyspace}', ColumnFamily='%{_table}'\\) %{data}: %{_onheap_total}\\/%{_offheap_total}, live: %{_onheap_live}\\/%{_offheap_live}, flushing: %{_onheap_flush}\\/%{_offheap_flush}, this: %{_onheap_this}\\/%{_offheap_this}",
                    "(?s)%{_prefix} %{regex(\"Enqueuing\"):db.operation}.* of %{_keyspace}: %{_onheap_bytes}%{data} \\(%{_onheap_pct}%\\) on-heap, %{_offheap_bytes} \\(%{_offheap_pct}%\\).*",
                    "(?s)%{_prefix} %{regex(\"Writing\"):db.operation}.*-%{_keyspace}%{data}\\(%{number:cassandra.bytes:scale(1000000)}%{data}, %{integer:cassandra.ops} ops, %{_onheap_pct}%\\/%{_offheap_pct}.*",
                    "(?s)%{_prefix} Completed %{regex(\"flushing\"):db.operation} %{_sstable} \\(%{number:cassandra.bytes_kb}KiB\\) for commitlog %{data:commitlog}",
                    "(?s)%{_prefix}\\s+%{regex(\"Compacted\"):db.operation}.* to \\[%{_sstable}\\].\\s+%{notSpace:cassandra.bytes_in} bytes to %{notSpace:cassandra.bytes_out} \\(\\~%{integer:cassandra.percent_of_orig}% of original\\) in %{notSpace:cassandra.duration_ms}ms = %{number:cassandra.speed_mb}MB/s.\\s+%{notSpace:cassandra.pkeys_in} total partitions merged to %{notSpace:cassandra.pkeys_out}\\.\\s+Partition merge counts were %{data:cassandra.merge_cnt}",
                    "(?s)%{_prefix} G.* %{integer:duration:scale(1000000)}ms. %{data}: %{integer:cassandra.eden.orig_bytes} -> %{integer:cassandra.eden.new_bytes}; %{data}: %{integer:cassandra.oldgen.orig_bytes} -> %{integer:cassandra.oldgen.new_bytes};.*",
                    "(?s)%{_prefix} %{word:cassandra.pool}\\s*(?>%{integer:cassandra.cache_used}\\s*%{integer:cassandra.cache_size}\\s*all|%{integer:cassandra.threads.active}\\s*%{integer:cassandra.threads.pending}\\s*%{integer:cassandra.threads.completed}\\s*%{integer:cassandra.threads.blocked}\\s*%{integer:cassandra.threads.all_time_blocked}|%{integer:cassandra.threads.active}\\s*%{integer:cassanadra.threads.pending})",
                    "(?s)%{_prefix} %{integer:db.operations} operations were slow in the last %{integer:elapsed_time:scale(1000000)} msecs:\\n%{data:db.slow_statements:array(\"\", \"\\\\n\")}",
                    "(?s)%{_prefix} %{data:msg}",
                ],
                aliases: {
                    "cassandra_compaction_key": "%{_prefix} %{regex(\"Compacting\"):db.operation}.* %{_keyspace}\\/%{_table}:%{data:partition_key} \\(%{_bytes} bytes\\)",
                    "cassandra_pool_cleaner": "%{_prefix} %{regex(\"Flushing\"):db.operation}.*\\(Keyspace='%{_keyspace}', ColumnFamily='%{_table}'\\) %{data}: %{_onheap_total}\\/%{_offheap_total}, live: %{_onheap_live}\\/%{_offheap_live}, flushing: %{_onheap_flush}\\/%{_offheap_flush}, this: %{_onheap_this}\\/%{_offheap_this}",
                    "cassandra_pool_cleaner2": "%{_prefix} %{regex(\"Enqueuing\"):db.operation}.* of %{_keyspace}: %{_onheap_bytes}%{data} \\(%{_onheap_pct}%\\) on-heap, %{_offheap_bytes} \\(%{_offheap_pct}%\\).*",
                    "cassandra_table_flush": "%{_prefix} %{regex(\"Writing\"):db.operation}.*-%{_keyspace}%{data}\\(%{number:cassandra.bytes:scale(1000000)}%{data}, %{integer:cassandra.ops} ops, %{_onheap_pct}%\\/%{_offheap_pct}.*",
                    "cassandra_mem_flush": "%{_prefix} Completed %{regex(\"flushing\"):db.operation} %{_sstable} \\(%{number:cassandra.bytes_kb}KiB\\) for commitlog %{data:commitlog}",
                    "cassandra_compaction": "%{_prefix}\\s+%{regex(\"Compacted\"):db.operation}.* to \\[%{_sstable}\\].\\s+%{notSpace:cassandra.bytes_in} bytes to %{notSpace:cassandra.bytes_out} \\(\\~%{integer:cassandra.percent_of_orig}% of original\\) in %{notSpace:cassandra.duration_ms}ms = %{number:cassandra.speed_mb}MB/s.\\s+%{notSpace:cassandra.pkeys_in} total partitions merged to %{notSpace:cassandra.pkeys_out}\\.\\s+Partition merge counts were %{data:cassandra.merge_cnt}",
                    "cassandra_gc_format": "%{_prefix} G.* %{integer:duration:scale(1000000)}ms. %{data}: %{integer:cassandra.eden.orig_bytes} -> %{integer:cassandra.eden.new_bytes}; %{data}: %{integer:cassandra.oldgen.orig_bytes} -> %{integer:cassandra.oldgen.new_bytes};.*",
                    "cassandra_thread_pending": "%{_prefix} %{word:cassandra.pool}\\s*(?>%{integer:cassandra.cache_used}\\s*%{integer:cassandra.cache_size}\\s*all|%{integer:cassandra.threads.active}\\s*%{integer:cassandra.threads.pending}\\s*%{integer:cassandra.threads.completed}\\s*%{integer:cassandra.threads.blocked}\\s*%{integer:cassandra.threads.all_time_blocked}|%{integer:cassandra.threads.active}\\s*%{integer:cassanadra.threads.pending})",
                    "cassandra_slow_statements": "%{_prefix} %{integer:db.operations} operations were slow in the last %{integer:elapsed_time:scale(1000000)} msecs:\\n%{data:db.slow_statements:array(\"\", \"\\\\n\")}",
                    "cassandra_fallback_parser": "%{_prefix} %{data:msg}",
                    "_level": "%{word:db.severity}",
                    "_thread_name": "%{notSpace:logger.thread_name}",
                    "_thread_id": "%{integer:logger.thread_id}",
                    "_logger_name": "%{notSpace:logger.name}",
                    "_table": "%{word:db.table}",
                    "_sstable": "%{notSpace:cassandra.sstable}",
                    "_bytes": "%{integer:cassandra.bytes}",
                    "_keyspace": "%{word:cassandra.keyspace}",
                    "_onheap_total": "%{number:cassandra.onheap.total}",
                    "_onheap_live": "%{number:cassandra.onheap.live}",
                    "_onheap_flush": "%{number:cassandra.onheap.flush}",
                    "_onheap_this": "%{number:cassandra.onheap.this}",
                    "_onheap_bytes": "%{integer:cassandra.onheap.bytes}",
                    "_onheap_pct": "%{integer:cassandra.onheap.percent}",
                    "_offheap_total": "%{number:cassandra.offheap.total}",
                    "_offheap_live": "%{number:cassandra.offheap.live}",
                    "_offheap_flush": "%{number:cassandra.offheap.flush}",
                    "_offheap_this": "%{number:cassandra.offheap.this}",
                    "_offheap_bytes": "%{integer:cassandra.offheap.bytes}",
                    "_offheap_pct": "%{integer:cassandra.offheap.percent}",
                    "_default_prefix": "%{_level}\\s+\\[(%{_thread_name}:%{_thread_id}|%{_thread_name})\\]\\s+%{date(\"yyyy-MM-dd HH:mm:ss,SSS\"):db.date}\\s+%{word:filename}.java:%{integer:lineno} -",
                    "_suggested_prefix": "%{date(\"yyyy-MM-dd HH:mm:ss\"):db.date} \\[(%{_thread_name}:%{_thread_id}|%{_thread_name})\\] %{_level} %{_logger_name}\\s+-",
                    "_prefix": "(?>%{_default_prefix}|%{_suggested_prefix})"
                }
            )
            if (err == null) {
                .custom, err = merge(.custom, custom, deep: true)
            }
        "#},
    },
    Source {
        name: "pipelines",
        target: "{}",
        program: indoc! {r#"
            status = string(.custom.http.status_category) ?? string(.custom.level) ?? ""
            status = downcase(status)
            if status == "" {
                .status = 6
            } else {
                if starts_with(status, "f") || starts_with(status, "emerg") {
                    .status = 0
                } else if starts_with(status, "a") {
                    .status = 1
                } else if starts_with(status, "c") {
                    .status = 2
                } else if starts_with(status, "e") {
                    .status = 3
                } else if starts_with(status, "w") {
                    .status = 4
                } else if starts_with(status, "n") {
                    .status = 5
                } else if starts_with(status, "i") {
                    .status = 6
                } else if starts_with(status, "d") || starts_with(status, "trace") || starts_with(status, "verbose") {
                    .status = 7
                } else if starts_with(status, "o") || starts_with(status, "s") || status == "ok" || status == "success" {
                    .status = 8
                }
            }
        "#},
    },
    Source {
        name: "add_bytes",
        target: "{}",
        program: indoc! {r#"
            . = "hello" + "world"
        "#},
    },
    Source {
        name: "add",
        target: "{}",
        program: indoc! {r#"
            . = 1 + 2
        "#},
    },
    Source {
        name: "derp",
        target: "{}",
        program: indoc! {r#"
            .foo = { "foo": 123 }
            .matches = { "num": "2", "name": .message }
        "#},
    },
    Source {
        name: "simple",
        target: "{}",
        program: indoc! {r#"
            .hostname = "vector"
            if .status == "warning" {
                .thing = upcase(.hostname)
            } else if .status == "notice" {
                .thung = downcase(.hostname)
            } else {
                .nong = upcase(.hostname)
            }

            .matches = { "name": .message, "num": "2" }
            .origin, .err = .hostname + "/" + .matches.name + "/" + .matches.num
        "#},
    },
    Source {
        name: "starts_with",
        target: "{}",
        program: indoc! {r#"
            status = string(.foo) ?? ""
            .status = starts_with("a", status)
        "#},
    },
    Source {
        name: "11",
        target: "{}",
        program: indoc! {r#"
            .hostname = "vector"
            if .status == "warning" {
                .thing = upcase(.hostname)
            } else if .status == "notice" {
                .thung = downcase(.hostname)
            } else {
                .nong = upcase(.hostname)
            }
        "#},
    },
    Source {
        name: "10",
        target: "{}",
        program: indoc! {r#"
            .foo = {
                "a": 123,
                "b": 456,
            }
        "#},
    },
    Source {
        name: "9",
        target: "{}",
        program: indoc! {r#"
            upcase("hi")
        "#},
    },
    Source {
        name: "8",
        target: "{}",
        program: indoc! {r#"
            123
        "#},
    },
    Source {
        name: "7",
        target: "{}",
        program: indoc! {r#"
            .foo == "hi"
        "#},
    },
    Source {
        name: "6",
        target: "{}",
        program: indoc! {r#"
            derp = "hi!"
        "#},
    },
    Source {
        name: "5",
        target: "{}",
        program: indoc! {r#"
            .derp = "hi!"
        "#},
    },
    Source {
        name: "4",
        target: "{}",
        program: indoc! {r#"
            .derp
        "#},
    },
    Source {
        name: "3",
        target: "{}",
        program: indoc! {r#"
            .
        "#},
    },
    Source {
        name: "parse_json",
        target: r#"
            {
                "hostname": "vector",
                "timestamp": "2022-05-10T10:43:15Z"
            }"#,
        program: indoc! {r#"
            parse_json!(s'{"noog": "nork"}')
        "#},
    },
    Source {
        name: "deletions",
        target: r#"{
            "hostname": "prod-223",
            "kubernetes": {
                "container_id": "a6926c9e-a4a0-4f80-8f71-2e7dd7d59f67",
                "container_image": "gcr.io/k8s-minikube/storage-provisioner:v3",
                "namespace_labels": {
                    "kubernetes.io/metadata.name": "kube-system"
                },
                "pod_annotations": {
                    "annotation1": "sample text",
                    "annotation2": "sample text"
                },
                "pod_ip": "192.168.1.1",
                "pod_name": "storage-provisioner",
                "pod_node_name": "minikube",
                "pod_owner": "root",
                "pod_uid": "93bde4d0-9731-4785-a80e-cd27ba8ad7c2",
                "pod_labels": {
                    "addonmanager.kubernetes.io/mode": "Reconcile",
                    "gcp-auth-skip-secret": "true",
                    "integration-test": "storage-provisioner",
                    "app": "production-123"
                }
            },
            "file": "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log",
            "message": "F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable",
            "source_type": "kubernetes_logs",
            "stream": "stderr",
            "timestamp": "2020-10-15T11:01:46.499555308Z"
        }"#,
        program: indoc! {r#"
            if exists(.kubernetes) {
                del(.kubernetes.container_id)
                del(.kubernetes.container_image)
                del(.kubernetes.namespace_labels)
                del(.kubernetes.pod_annotations)
                del(.kubernetes.pod_ip)
                del(.kubernetes.pod_name)
                del(.kubernetes.pod_node_name)
                del(.kubernetes.pod_owner)
                del(.kubernetes.pod_uid)
                del(.kubernetes.pod_labels.app)
            }
        "#},
    },
    Source {
        name: "0",
        target: "{}",
        program: indoc! {r#"
            uuid_v4()
        "#},
    },
    Source {
        name: "simple",
        target: "{}",
        program: indoc! {r#"
            .hostname = "vector"

            if .status == "warning" {
                .thing = upcase(.hostname)
            } else if .status == "notice" {
                .thung = downcase(.hostname)
            } else {
                .nong = upcase(.hostname)
            }

            .matches = { "name": .message, "num": "2" }
            .origin, .err = .hostname + "/" + .matches.name + "/" + .matches.num
        "#},
    },
];

#[inline(never)]
#[no_mangle]
pub extern "C" fn derp() {
    println!("derp'n");
}

fn benchmark_vrl_runtimes(c: &mut Criterion) {
    derp();
    downcase(&mut Value::Null, &mut Ok(Value::Null));
    unsafe { vrl_fn_uuid_v4(&mut Ok(Value::Null)) };
    unsafe { vrl_fn_upcase(&mut Ok(Value::Null), &mut Ok(Value::Null)) };
    upcase(&mut Value::Null, &mut Ok(Value::Null));

    /*
    {
        use inkwell::context::Context;
        use inkwell::targets::{InitializationConfig, Target};
        use inkwell::OptimizationLevel;
        Target::initialize_native(&InitializationConfig::default()).unwrap();
        let context = Context::create();
        let module = context.create_module("test");
        let builder = context.create_builder();

        // Set up the function signature
        let double = context.f64_type();
        let sig = double.fn_type(&[], false);

        // Add the function to our module
        let f = module.add_function("test_fn", sig, None);
        let b = context.append_basic_block(f, "entry");
        builder.position_at_end(b);

        let function_name = "derp".to_owned();
        let function_type = context.void_type().fn_type(&[], false);
        let fn_impl = module.add_function(&function_name, function_type, None);
        builder.build_call(fn_impl, &[], &function_name);

        {
            let function_name = "vrl_fn_uuid_v4".to_owned();
            let function_type = context.void_type().fn_type(&[], false);
            let fn_impl = module.add_function(&function_name, function_type, None);
            builder.build_call(fn_impl, &[], &function_name);
        }

        // Insert a return statement
        let ret = double.const_float(64.0);
        builder.build_return(Some(&ret));

        println!("{}", module.print_to_string().to_string());

        // create the JIT engine
        let mut ee = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .unwrap();

        // fetch our JIT'd function and execute it
        unsafe {
            let test_fn = ee
                .get_function::<unsafe extern "C" fn() -> f64>("test_fn")
                .unwrap();
            let return_value = test_fn.call();
            assert_eq!(return_value, 64.0);
        }
    }
    */

    let mut group = c.benchmark_group("vrl/runtime");
    for source in SOURCES {
        let state = state::Runtime::default();
        let runtime = Runtime::new(state);
        let tz = TimeZone::default();
        let mut external_env = state::ExternalEnv::default();
        let (program, mut local_env, _) =
            vrl::compile_with_state(source.program, &vrl_stdlib::all(), &mut external_env).unwrap();
        let vm = runtime
            .compile(vrl_stdlib::all(), &program, &mut external_env)
            .unwrap();
        let builder = compiler::llvm::Compiler::new().unwrap();
        println!("bench 1");
        let mut symbols = HashMap::new();
        symbols.insert("vrl_fn_downcase", vrl_stdlib::vrl_fn_downcase as usize);
        symbols.insert("vrl_fn_merge", vrl_stdlib::vrl_fn_merge as usize);
        symbols.insert(
            "vrl_fn_parse_groks",
            vrl_stdlib::vrl_fn_parse_groks as usize,
        );
        symbols.insert(
            "vrl_fn_starts_with",
            vrl_stdlib::vrl_fn_starts_with as usize,
        );
        symbols.insert("vrl_fn_string", vrl_stdlib::vrl_fn_string as usize);
        symbols.insert("vrl_fn_upcase", vrl_stdlib::vrl_fn_upcase as usize);
        let library = builder
            .compile(
                (&mut local_env, &mut external_env),
                &program,
                vrl_stdlib::all(),
                symbols,
            )
            .unwrap();
        println!("bench 2");
        let execute = library.get_function().unwrap();
        println!("bench 3");

        {
            println!("yo");
            let mut target: Value = serde_json::from_str(source.target).expect("valid json");
            let mut context = core::Context {
                target: &mut target,
                timezone: &tz,
            };
            let mut result = Ok(Value::Null);
            println!("bla");
            unsafe { execute.call(&mut context, &mut result) };
            println!("derp");
        }

        {
            let mut target: Value = serde_json::from_str(source.target).expect("valid json");
            let mut context = core::Context {
                target: &mut target,
                timezone: &tz,
            };
            let mut result = Ok(Value::Null);
            unsafe { execute.call(&mut context, &mut result) };

            println!("LLVM target: {}", target);
            println!("LLVM result: {:?}", result);
        }

        {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            let mut target: Value = serde_json::from_str(source.target).expect("valid json");
            let result = runtime.run_vm(&vm, &mut target, &tz);
            runtime.clear();

            println!("VM target: {}", target);
            println!("VM result: {:?}", result);
        }

        {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            let mut target: Value = serde_json::from_str(source.target).expect("valid json");
            let result = runtime.resolve(&mut target, &program, &tz);
            runtime.clear();

            println!("AST target: {}", target);
            println!("AST result: {:?}", result);
        }

        group.bench_with_input(
            BenchmarkId::new("LLVM", source.name),
            &execute,
            |b, execute| {
                let target: Value = serde_json::from_str(source.target).expect("valid json");

                b.iter_with_setup(
                    || target.clone(),
                    |mut obj| {
                        {
                            let mut context = core::Context {
                                target: &mut obj,
                                timezone: &tz,
                            };
                            let mut result = Ok(Value::Null);
                            unsafe { execute.call(&mut context, &mut result) };
                        }
                        obj // Return the obj so it doesn't get dropped.
                    },
                )
            },
        );

        group.bench_with_input(BenchmarkId::new("VM", source.name), &vm, |b, vm| {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            let target: Value = serde_json::from_str(source.target).expect("valid json");

            b.iter_with_setup(
                || target.clone(),
                |mut obj| {
                    let _ = black_box(runtime.run_vm(vm, &mut obj, &tz));
                    runtime.clear();
                    obj
                },
            )
        });

        group.bench_with_input(BenchmarkId::new(source.name, "ast"), &(), |b, _| {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            let target: Value = serde_json::from_str(source.target).expect("valid json");

            b.iter_with_setup(
                || target.clone(),
                |mut obj| {
                    let _ = black_box(runtime.resolve(&mut obj, &program, &tz));
                    runtime.clear();
                    obj
                },
            )
        });
    }
}

criterion_group!(name = vrl_runtime;
                config = Criterion::default()
                    .warm_up_time(Duration::from_secs(5))
                    .measurement_time(Duration::from_secs(30))
                    // degree of noise to ignore in measurements, here 1%
                    .noise_threshold(0.01)
                    // likelihood of noise registering as difference, here 5%
                    .significance_level(0.05)
                    // likelihood of capturing the true runtime, here 95%
                    .confidence_level(0.95)
                    // total number of bootstrap resamples, higher is less noisy but slower
                    .nresamples(100_000)
                    // total samples to collect within the set measurement time
                    .sample_size(150);
                 targets = benchmark_vrl_runtimes);
criterion_main!(vrl_runtime);
