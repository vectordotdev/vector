Improved throughput and memory use for log pipelines by updating VRL, which now stores event fields in a
compact sorted vector with small keys stored inline. In regression benchmarks, remap-heavy and syslog
pipelines process up to ~20% more events per CPU, with 10-20% lower peak memory.

authors: lukesteensen
