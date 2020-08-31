# RFC XXXX - 2020-08-28 - Collecting metrics from the JVM via JMX

This RFC is to introduce a new metrics source to consume metrics from Java Virtual Machines (JVM) using the JMX protocol. The high level plan is to implement one source that collects metrics from JVM servers.

Background reading on JVM/JMX monitoring:

- https://sysdig.com/blog/jmx-monitoring-custom-metrics/

## Scope

This RFC will cover:

- A new source for JVM-based metrics using JMX.

## Motivation

Users want to collect, transform, and forward metrics to better observe how their JVM-based applications are performing.

## Internal Proposal

Build a single source called `jmx_metrics` (name to be confirmed) to collect JVM metrics.

The recommended implementation is to use the Rust JMX client to connect to a JVM server by an address specified in configuration.

- https://docs.rs/jmx/0.2.1/jmx/index.html
- https://github.com/stefano-pogliani/jmx-rust


And return these metrics by parsing the query results and converting them into metrics using the database name and column names.

- `jmx_up` -> Used as an uptime metric (0/1) ? - merits a broader discussion.
- `jmx_config_reload_success_total` (counter)
- `process_cpu_seconds_total` (counter)
- `process_start_time_seconds` (gauge)
- `process_open_fds` (gauge)
- `process_max_fds` (gauge)
- `jvm_threads_current` (gauge)
- `jvm_threads_daemon` (gauge)
- `jvm_threads_peak` (gauge)
- `jvm_threads_started_total` (counter)
- `jvm_threads_deadlocked` (gauge)
- `jvm_threads_deadlocked_monitor` (gauge)
- `jvm_threads_state` tagged with `state` (gauge)
- `jmx_config_reload_failure_total` (counter)
- `jvm_buffer_pool_used_bytes` tagged with `pool` (gauge)
- `jvm_buffer_pool_capacity_bytes` tagged with `pool` (gauge)
- `jvm_buffer_pool_used_buffers` tagged with `pool` (gauge)
- `jvm_classes_loaded` (gauge)
- `jvm_classes_loaded_total` (counter)
- `jvm_classes_unloaded_total` (counter)
- `java_lang_MemoryPool_UsageThresholdSupported` tagged with `name` (gauge)
- `java_lang_Threading_ThreadContentionMonitoringEnabled` (gauge)
- `java_lang_OperatingSystem_CommittedVirtualMemorySize` (gauge)
- `java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used` tagged with `name`, `key` (counter?)
- `java_lang_Threading_ThreadContentionMonitoringSupported` (gauge)
- `jvm_gc_collection_seconds` tagged with `gc` (summary)
- `jvm_memory_bytes_used` tagged with `area` (gauge)
- `jvm_memory_bytes_committed` tagged with `area` (gauge)
- `jvm_memory_bytes_max` tagged with `area` (gauge)
- `jvm_memory_bytes_init` tagged with `area` (gauge)
- `jvm_memory_pool_bytes_used` tagged with `pool` (gauge)
- `jvm_memory_pool_bytes_committed` tagged with `pool` (gauge)
- `jvm_memory_pool_bytes_max` tagged with `pool` (gauge)
-`jvm_memory_pool_bytes_init` tagged with `pool` (gauge)
- `jvm_info` tagged with version, vendor, runtime (gauge)
- `jvm_memory_pool_allocated_bytes_total` tagged with `pool` (counter)

# TYPE java_lang_Memory_HeapMemoryUsage_committed untyped
java_lang_Memory_HeapMemoryUsage_committed 5.74619648E8

# TYPE java_lang_OperatingSystem_TotalSwapSpaceSize untyped
java_lang_OperatingSystem_TotalSwapSpaceSize 1.3958643712E10

# TYPE java_lang_MemoryPool_CollectionUsage_max tagged with `name` (untyped)
# TYPE java_lang_Runtime_StartTime untyped
# TYPE java_lang_GarbageCollector_LastGcInfo_endTime tagged with `name` (untyped)
# TYPE java_lang_Memory_HeapMemoryUsage_max untyped
# TYPE java_lang_MemoryPool_UsageThreshold tagged with `name` (untyped)
# TYPE java_lang_MemoryPool_CollectionUsageThresholdCount tagged with `name` (untyped)
# TYPE java_lang_Memory_NonHeapMemoryUsage_used (untyped)
# TYPE java_lang_Threading_PeakThreadCount (untyped)
# TYPE java_lang_MemoryPool_PeakUsage_used tagged with `name` (untyped)
# TYPE java_lang_ClassLoading_TotalLoadedClassCount (untyped)
# TYPE java_lang_OperatingSystem_MaxFileDescriptorCount (untyped)
# TYPE java_lang_ClassLoading_Verbose (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_id tagged with `name` (untyped)
# TYPE java_lang_Threading_CurrentThreadUserTime untyped
# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed tagged with `name` and `key` (untyped)
# TYPE java_lang_Threading_ThreadCount untyped
java_lang_Threading_ThreadCount 24.0

# TYPE java_lang_MemoryPool_PeakUsage_committed tagged with `name` (untyped)
# TYPE java_lang_Memory_ObjectPendingFinalizationCount untyped
java_lang_Memory_ObjectPendingFinalizationCount 0.0

# TYPE java_lang_MemoryPool_Usage_used tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_CollectionCount tagged with `name` (untyped)
# TYPE java_lang_Threading_SynchronizerUsageSupported untyped
java_lang_Threading_SynchronizerUsageSupported 1.0

# TYPE java_lang_Runtime_BootClassPathSupported untyped
java_lang_Runtime_BootClassPathSupported 1.0

# TYPE java_nio_BufferPool_Count tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_GcThreadCount tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed tagged with `name` and `key` (untyped)
# TYPE java_lang_Threading_CurrentThreadCpuTimeSupported untyped
java_lang_Threading_CurrentThreadCpuTimeSupported 1.0

# TYPE java_lang_ClassLoading_LoadedClassCount untyped
java_lang_ClassLoading_LoadedClassCount 10497.0

# TYPE java_lang_MemoryPool_CollectionUsage_init tagged with `name` (untyped)

# TYPE java_lang_MemoryPool_PeakUsage_max tagged with `name` (untyped)

# TYPE java_lang_MemoryPool_Usage_max tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_Valid tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used tagged with `name` and `key` (untyped)
# TYPE java_lang_Threading_ThreadAllocatedMemoryEnabled untyped
java_lang_Threading_ThreadAllocatedMemoryEnabled 1.0

# TYPE java_lang_MemoryManager_Valid tagged with `name` (untyped)
# TYPE java_lang_MemoryPool_Usage_init tagged with `name` (untyped)
# TYPE java_lang_OperatingSystem_ProcessCpuLoad untyped
java_lang_OperatingSystem_ProcessCpuLoad 0.0

# TYPE java_lang_MemoryPool_CollectionUsage_committed tagged with `name` (untyped)
# TYPE java_lang_OperatingSystem_TotalPhysicalMemorySize untyped
java_lang_OperatingSystem_TotalPhysicalMemorySize 3.4359738368E10

# TYPE java_lang_Memory_NonHeapMemoryUsage_committed untyped
java_lang_Memory_NonHeapMemoryUsage_committed 7.7971456E7

# TYPE java_lang_Compilation_TotalCompilationTime untyped
java_lang_Compilation_TotalCompilationTime 4971.0

# TYPE java_lang_Memory_Verbose untyped
java_lang_Memory_Verbose 0.0

# TYPE java_lang_MemoryPool_Valid tagged with `name` (untyped)
# TYPE java_lang_OperatingSystem_FreeSwapSpaceSize untyped
java_lang_OperatingSystem_FreeSwapSpaceSize 1.410596864E9

# TYPE java_lang_MemoryPool_UsageThresholdExceeded tagged with `name` (untyped)
# TYPE java_lang_Threading_CurrentThreadCpuTime untyped
java_lang_Threading_CurrentThreadCpuTime 8.008E7

# TYPE java_lang_MemoryPool_CollectionUsageThreshold tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_CollectionTime tagged with `name` (untyped)
# TYPE java_lang_Compilation_CompilationTimeMonitoringSupported untyped
java_lang_Compilation_CompilationTimeMonitoringSupported 1.0

# TYPE java_lang_MemoryPool_Usage_committed tagged with `name` (untyped)
# TYPE java_lang_Memory_NonHeapMemoryUsage_init untyped
java_lang_Memory_NonHeapMemoryUsage_init 2555904.0

# TYPE java_lang_MemoryPool_PeakUsage_init tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_startTime tagged with `name` (untyped)
# TYPE java_lang_OperatingSystem_AvailableProcessors untyped
java_lang_OperatingSystem_AvailableProcessors 8.0

# TYPE java_lang_MemoryPool_CollectionUsageThresholdSupported tagged with `name` (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max tagged with `name` and `key` (untyped)
# TYPE java_lang_ClassLoading_UnloadedClassCount untyped
java_lang_ClassLoading_UnloadedClassCount 0.0

# TYPE java_nio_BufferPool_MemoryUsed tagged with `name` (untyped)
# TYPE java_nio_BufferPool_TotalCapacity tagged with `name` (untyped)
# TYPE java_lang_Memory_HeapMemoryUsage_used untyped
java_lang_Memory_HeapMemoryUsage_used 1.3891992E8

# TYPE java_lang_MemoryPool_CollectionUsage_used tagged with `name` (untyped)
# TYPE java_lang_Memory_HeapMemoryUsage_init untyped
java_lang_Memory_HeapMemoryUsage_init 5.36870912E8

# TYPE java_lang_OperatingSystem_SystemCpuLoad untyped
java_lang_OperatingSystem_SystemCpuLoad 0.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init tagged with `name` and `keys` (untyped)
# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init tagged with `name` and `key` (untyped)

# TYPE java_lang_Threading_ThreadAllocatedMemorySupported untyped
java_lang_Threading_ThreadAllocatedMemorySupported 1.0

# TYPE java_lang_Memory_NonHeapMemoryUsage_max untyped
java_lang_Memory_NonHeapMemoryUsage_max -1.0

# TYPE java_lang_Threading_DaemonThreadCount untyped
java_lang_Threading_DaemonThreadCount 7.0

# TYPE java_lang_Threading_ThreadCpuTimeSupported untyped
java_lang_Threading_ThreadCpuTimeSupported 1.0

# TYPE java_lang_OperatingSystem_SystemLoadAverage untyped
java_lang_OperatingSystem_SystemLoadAverage 2.359375

# TYPE java_lang_Threading_TotalStartedThreadCount untyped
java_lang_Threading_TotalStartedThreadCount 26.0

# TYPE java_lang_OperatingSystem_ProcessCpuTime untyped
java_lang_OperatingSystem_ProcessCpuTime 8.899873E9

# TYPE java_lang_OperatingSystem_FreePhysicalMemorySize untyped
java_lang_OperatingSystem_FreePhysicalMemorySize 1.45588224E8

# TYPE java_lang_Runtime_Uptime untyped
java_lang_Runtime_Uptime 27871.0

# TYPE java_lang_MemoryPool_CollectionUsageThresholdExceeded tagged with `name` (untyped)

# TYPE java_lang_GarbageCollector_LastGcInfo_duration tagged with `name` (untyped)
# TYPE java_lang_Threading_ObjectMonitorUsageSupported untyped
java_lang_Threading_ObjectMonitorUsageSupported 1.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max tagged with `name` and `key` (untyped)
# TYPE java_lang_MemoryPool_UsageThresholdCount tagged with `name` (untyped)
# TYPE java_lang_Threading_ThreadCpuTimeEnabled untyped
java_lang_Threading_ThreadCpuTimeEnabled 1.0

# TYPE java_lang_OperatingSystem_OpenFileDescriptorCount untyped
java_lang_OperatingSystem_OpenFileDescriptorCount 174.0




Naming of metrics is determined via:

- `jmx _ metric_name`

This is in line with the Prometheus naming convention for their exporter.

## Doc-level Proposal

The following additional source configuration will be added:

```toml
[sources.my_source_id]
  type = "postgresql_metrics" # required
  address = "postgres://postgres@localhost" # required - address of the PG server.
  databases = ["production", "testing"] # optional, list of databases to query. Defaults to all if not specified.
  scrape_interval_secs = 15 # optional, default, seconds
  namespace = "postgresql" # optional, default is "postgresql", namespace to put metrics under
```

- We'd also add a guide for doing this without root permissions.

## Rationale

The JVM is a popular platform for running applications. Additionally, it is the basis for running a number of other infrastructure tools, including Kafka and Cassandra. User frequently want to

Additionally, as part of Vector's vision to be the "one tool" for ingesting and shipping observability data, it makes sense to add as many sources as possible to reduce the likelihood that a user will not be able to ingest metrics from their tools.

## Prior Art

- https://github.com/prometheus/jmx_exporter
- https://github.com/influxdata/telegraf/tree/master/plugins/inputs/jolokia
- https://github.com/influxdata/telegraf/tree/master/plugins/inputs/jolokia2
- https://collectd.org/wiki/index.php/Plugin:GenericJMX
- https://collectd.org/documentation/manpages/collectd-java.5.shtml#genericjmx_plugin
- https://github.com/ScalaConsultants/panopticon-tui
- https://github.com/replicante-io/agents/tree/c62821b45b1c44bc3e5f2bfec6ebfe4454c694f1/agents/kafka


## Drawbacks

- Additional maintenance and integration testing burden of a new source

## Alternatives

### Having users run telegraf or Prom node exporter and using Vector's prometheus source to scrape it

We could not add the source directly to Vector and instead instruct users to run Prometheus' `jmx_exporter` and point Vector at the resulting data.

I decided against this for two reasons:

a)
b) As it would be in contrast with one of the listed
principles of Vector:

> One Tool. All Data. - One simple tool gets your logs, metrics, and traces
> (coming soon) from A to B.

[Vector principles](https://vector.dev/docs/about/what-is-vector/#who-should-use-vector)

If users are already running Prometheus though, they could opt for this path.

## Outstanding Questions

- ???

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with the initial source implementation

## Future Work

- ???
