###
# This script matches criterion output to output JSON for use for formatting output in CI
#
# We'd prefer to use machine output from cargo-criterion but there is an
# outstanding issue preventing this:
# https://github.com/bheisler/cargo-criterion/issues/17
#
# It matches output like:
# ```
# Benchmarking transforms/transforms
# Benchmarking transforms/transforms: Warming up for 3.0000 s
# Benchmarking transforms/transforms: Collecting 100 samples in estimated 7.6949 s (200 iterations)
# Benchmarking transforms/transforms: Analyzing
# transforms/transforms   time:   [31.562 ms 31.932 ms 32.307 ms]
#                         thrpt:  [32.471 MiB/s 32.852 MiB/s 33.238 MiB/s]
#                  change:
#                         time:   [-0.5510% +1.0918% +2.8006%] (p = 0.19 > 0.05)
#                         thrpt:  [-2.7243% -1.0800% +0.5540%]
#                         No change in performance detected.
# Found 2 outliers among 100 measurements (2.00%)
#   1 (1.00%) low mild
#   1 (1.00%) high severe
#
# Benchmarking complex/complex
# Benchmarking complex/complex: Warming up for 3.0000 s
#
# Warning: Unable to complete 100 samples in 5.0s. You may wish to increase target time to 182.3s, or reduce sample count to 10.
# Benchmarking complex/complex: Collecting 100 samples in estimated 182.26 s (100 iterations)
# Benchmarking complex/complex: Analyzing
# complex/complex         time:   [1.8272 s 1.8391 s 1.8498 s]
#                         change: [+0.1481% +0.8758% +1.5410%] (p = 0.01 < 0.05)
#                         Change within noise threshold.
# Found 5 outliers among 100 measurements (5.00%)
#   3 (3.00%) low severe
#   2 (2.00%) low mild
#
# ```
#
# To output records like:
# ```
# { "throughput": "32.852 MiB/s",  "change": "none",  "throughput_change": "-1.0800%",  "name": "transforms/transforms",  "time_change": "+1.0918%",  "time": "31.932 ms", "p": "0.00"}
# { "throughput": null,  "change": "none",  "throughput_change": null,  "name": "complex/complex",  "time_change": "+0.8758%",  "time": "1.8391 s", "p": "0.02"}
# ```
###

BEGIN {
  # match time:   [1.8272 s 1.8391 s 1.8498 s]
  measurement_regex = "\\[(\\S+ \\S+) (\\S+ \\S+) (\\S+ \\S+)\\]"
  # match time change: [+0.1481% +0.8758% +1.5410%] (p = 0.01 < 0.05)
  time_change_regex = "\\[(\\S+) (\\S+) (\\S+)\\] \\(p = (\\S+) [<>] \\S+\\)"
  # match thrpt change: [+0.1481% +0.8758% +1.5410%]
  thrpt_change_regex = "\\[(\\S+) (\\S+) (\\S+)\\]"
}

function finish_benchmark(benchmark, change)
{
  benchmark["change"] = change
  separator = ""
  printf "{"
  for (field in benchmark) {
    value = benchmark[field]
    gsub(/\\/, "\\\\", value)
    gsub(/"/, "\\\"", value)
    if (value != "null") value = "\""value"\""

    field = "\""field"\""

    printf "%s %s: %s", separator, field, value
    separator = ", "
  }
  printf "}\n"
  delete benchmark
  in_change = 0
  in_bench = 0
}

/^Benchmarking .*: Analyzing$/ {
  match($0, /^Benchmarking (.*): Analyzing$/, arr)
  num_fields = split("name time time_change throughput throughput_change p change", fields)
  for (i=1; i<=num_fields; i++) benchmark[fields[i]] = "null"
  benchmark["name"] = arr[1]
  benchmark["change"] = "unknown"
  in_bench = 1
}

in_bench && /change:$/ { in_change = 1 }

in_change && /time: / {
  match($0, time_change_regex, arr)
  benchmark["time_change"] = arr[2]
  benchmark["p"] = arr[4]
}

in_change && /\s+thrpt: / {
  match($0, thrpt_change_regex, arr)
  benchmark["throughput_change"] = arr[2]
}

!in_change && /time: / {
  match($0, measurement_regex, arr)
  benchmark["time"] = arr[2]
}

!in_change && /thrpt: / {
  match($0, measurement_regex, arr)
  benchmark["throughput"] = arr[2]
}

!in_change && /change: / {
  match($0, time_change_regex, arr)
  benchmark["time_change"] = arr[2]
  benchmark["p"] = arr[4]
}

in_bench && /Performance has improved./ { finish_benchmark(benchmark, "improved") }
in_bench && /Performance has regressed./ { finish_benchmark(benchmark, "regressed") }
in_bench && /Change within noise threshold./ { finish_benchmark(benchmark, "none") }
in_bench && /No change in performance detected./ { finish_benchmark(benchmark, "none") }
