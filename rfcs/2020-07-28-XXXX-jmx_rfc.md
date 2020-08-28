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


# TYPE jmx_exporter_build_info gauge
jmx_exporter_build_info{version="0.13.0",name="jmx_prometheus_javaagent",} 1.0

# TYPE jmx_config_reload_success_total counter
jmx_config_reload_success_total 0.0

# TYPE process_cpu_seconds_total counter
process_cpu_seconds_total 8.696675

# TYPE process_start_time_seconds gauge
process_start_time_seconds 1.598618500925E9

# TYPE process_open_fds gauge
process_open_fds 174.0

# TYPE process_max_fds gauge
process_max_fds 10240.0

# TYPE jvm_threads_current gauge
jvm_threads_current 24.0

# TYPE jvm_threads_daemon gauge
jvm_threads_daemon 7.0

# TYPE jvm_threads_peak gauge
jvm_threads_peak 24.0

# TYPE jvm_threads_started_total counter
jvm_threads_started_total 26.0

# TYPE jvm_threads_deadlocked gauge
jvm_threads_deadlocked 0.0

# TYPE jvm_threads_deadlocked_monitor gauge
jvm_threads_deadlocked_monitor 0.0

# TYPE jvm_threads_state gauge
jvm_threads_state{state="WAITING",} 3.0
jvm_threads_state{state="RUNNABLE",} 7.0
jvm_threads_state{state="TIMED_WAITING",} 14.0
jvm_threads_state{state="TERMINATED",} 0.0
jvm_threads_state{state="NEW",} 0.0
jvm_threads_state{state="BLOCKED",} 0.0

# TYPE jmx_config_reload_failure_total counter
jmx_config_reload_failure_total 0.0

# TYPE jvm_buffer_pool_used_bytes gauge
jvm_buffer_pool_used_bytes{pool="direct",} 73729.0

# TYPE jvm_buffer_pool_capacity_bytes gauge
jvm_buffer_pool_capacity_bytes{pool="direct",} 73728.0

# TYPE jvm_buffer_pool_used_buffers gauge
jvm_buffer_pool_used_buffers{pool="direct",} 4.0

# TYPE jvm_classes_loaded gauge
jvm_classes_loaded 10370.0

# TYPE jvm_classes_loaded_total counter
jvm_classes_loaded_total 10370.0

# TYPE jvm_classes_unloaded_total counter
jvm_classes_unloaded_total 0.0

# TYPE java_lang_MemoryPool_UsageThresholdSupported untyped
java_lang_MemoryPool_UsageThresholdSupported{name="Metaspace",} 1.0
java_lang_MemoryPool_UsageThresholdSupported{name="PS Old Gen",} 1.0
java_lang_MemoryPool_UsageThresholdSupported{name="PS Eden Space",} 0.0
java_lang_MemoryPool_UsageThresholdSupported{name="Code Cache",} 1.0
java_lang_MemoryPool_UsageThresholdSupported{name="Compressed Class Space",} 1.0
java_lang_MemoryPool_UsageThresholdSupported{name="PS Survivor Space",} 0.0

# TYPE java_lang_Threading_ThreadContentionMonitoringEnabled untyped
java_lang_Threading_ThreadContentionMonitoringEnabled 0.0

# TYPE java_lang_OperatingSystem_CommittedVirtualMemorySize untyped
java_lang_OperatingSystem_CommittedVirtualMemorySize 1.5949647872E10

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS Scavenge",key="Compressed Class Space",} 8621048.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS Scavenge",key="PS Survivor Space",} 1.8829768E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS Scavenge",key="PS Old Gen",} 2.3152984E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS Scavenge",key="Metaspace",} 4.6086976E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS Scavenge",key="PS Eden Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS Scavenge",key="Code Cache",} 1.126112E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS MarkSweep",key="Compressed Class Space",} 6590808.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS MarkSweep",key="PS Survivor Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS MarkSweep",key="PS Old Gen",} 2.3144792E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS MarkSweep",key="Metaspace",} 3.576556E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS MarkSweep",key="PS Eden Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_used{name="PS MarkSweep",key="Code Cache",} 8525504.0

# TYPE java_lang_OperatingSystem_TotalSwapSpaceSize untyped
java_lang_OperatingSystem_TotalSwapSpaceSize 1.3958643712E10

# TYPE java_lang_Threading_ThreadContentionMonitoringSupported untyped
java_lang_Threading_ThreadContentionMonitoringSupported 1.0

# TYPE java_lang_Memory_HeapMemoryUsage_committed untyped
java_lang_Memory_HeapMemoryUsage_committed 5.74619648E8

# TYPE java_lang_MemoryPool_CollectionUsage_max untyped
java_lang_MemoryPool_CollectionUsage_max{name="PS Old Gen",} 5.726797824E9
java_lang_MemoryPool_CollectionUsage_max{name="PS Eden Space",} 2.819096576E9
java_lang_MemoryPool_CollectionUsage_max{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_Runtime_StartTime untyped
java_lang_Runtime_StartTime 1.598618500925E12

# TYPE java_lang_GarbageCollector_LastGcInfo_endTime untyped
java_lang_GarbageCollector_LastGcInfo_endTime{name="PS Scavenge",} 2883.0
java_lang_GarbageCollector_LastGcInfo_endTime{name="PS MarkSweep",} 2135.0

# TYPE java_lang_Memory_HeapMemoryUsage_max untyped
java_lang_Memory_HeapMemoryUsage_max 7.635730432E9

# TYPE java_lang_MemoryPool_UsageThreshold untyped
java_lang_MemoryPool_UsageThreshold{name="Metaspace",} 0.0
java_lang_MemoryPool_UsageThreshold{name="PS Old Gen",} 0.0
java_lang_MemoryPool_UsageThreshold{name="Code Cache",} 0.0
java_lang_MemoryPool_UsageThreshold{name="Compressed Class Space",} 0.0

# TYPE java_lang_MemoryPool_CollectionUsageThresholdCount untyped
java_lang_MemoryPool_CollectionUsageThresholdCount{name="PS Old Gen",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdCount{name="PS Eden Space",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdCount{name="PS Survivor Space",} 0.0

# TYPE java_lang_Memory_NonHeapMemoryUsage_used untyped
java_lang_Memory_NonHeapMemoryUsage_used 7.5478816E7

# TYPE java_lang_Threading_PeakThreadCount untyped
java_lang_Threading_PeakThreadCount 24.0

# TYPE java_lang_MemoryPool_PeakUsage_used untyped
java_lang_MemoryPool_PeakUsage_used{name="Metaspace",} 5.3865496E7
java_lang_MemoryPool_PeakUsage_used{name="PS Old Gen",} 2.3152984E7
java_lang_MemoryPool_PeakUsage_used{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_PeakUsage_used{name="Code Cache",} 1.269664E7
java_lang_MemoryPool_PeakUsage_used{name="Compressed Class Space",} 1.0043928E7
java_lang_MemoryPool_PeakUsage_used{name="PS Survivor Space",} 1.8829768E7

# TYPE java_lang_ClassLoading_TotalLoadedClassCount untyped
java_lang_ClassLoading_TotalLoadedClassCount 10497.0

# TYPE java_lang_OperatingSystem_MaxFileDescriptorCount untyped
java_lang_OperatingSystem_MaxFileDescriptorCount 10240.0

# TYPE java_lang_ClassLoading_Verbose untyped
java_lang_ClassLoading_Verbose 0.0

# TYPE java_lang_GarbageCollector_LastGcInfo_id untyped
java_lang_GarbageCollector_LastGcInfo_id{name="PS Scavenge",} 3.0
java_lang_GarbageCollector_LastGcInfo_id{name="PS MarkSweep",} 2.0

# TYPE java_lang_Threading_CurrentThreadUserTime untyped
java_lang_Threading_CurrentThreadUserTime 7.253E7

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS Scavenge",key="Compressed Class Space",} 8871936.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS Scavenge",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS Scavenge",key="PS Old Gen",} 4.17857536E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS Scavenge",key="Metaspace",} 4.6882816E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS Scavenge",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS Scavenge",key="Code Cache",} 1.1337728E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS MarkSweep",key="Compressed Class Space",} 6905856.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS MarkSweep",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS MarkSweep",key="PS Old Gen",} 4.17857536E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS MarkSweep",key="Metaspace",} 3.6265984E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS MarkSweep",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_committed{name="PS MarkSweep",key="Code Cache",} 8585216.0

# TYPE java_lang_Threading_ThreadCount untyped
java_lang_Threading_ThreadCount 24.0

# TYPE java_lang_MemoryPool_PeakUsage_committed untyped
java_lang_MemoryPool_PeakUsage_committed{name="Metaspace",} 5.4747136E7
java_lang_MemoryPool_PeakUsage_committed{name="PS Old Gen",} 4.17857536E8
java_lang_MemoryPool_PeakUsage_committed{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_PeakUsage_committed{name="Code Cache",} 1.277952E7
java_lang_MemoryPool_PeakUsage_committed{name="Compressed Class Space",} 1.04448E7
java_lang_MemoryPool_PeakUsage_committed{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_Memory_ObjectPendingFinalizationCount untyped
java_lang_Memory_ObjectPendingFinalizationCount 0.0

# TYPE java_lang_MemoryPool_Usage_used untyped
java_lang_MemoryPool_Usage_used{name="Metaspace",} 5.3874128E7
java_lang_MemoryPool_Usage_used{name="PS Old Gen",} 2.3152984E7
java_lang_MemoryPool_Usage_used{name="PS Eden Space",} 9.4907688E7
java_lang_MemoryPool_Usage_used{name="Code Cache",} 1.1496256E7
java_lang_MemoryPool_Usage_used{name="Compressed Class Space",} 1.0043928E7
java_lang_MemoryPool_Usage_used{name="PS Survivor Space",} 1.8829768E7

# TYPE java_lang_GarbageCollector_CollectionCount untyped
java_lang_GarbageCollector_CollectionCount{name="PS Scavenge",} 3.0
java_lang_GarbageCollector_CollectionCount{name="PS MarkSweep",} 2.0

# TYPE java_lang_Threading_SynchronizerUsageSupported untyped
java_lang_Threading_SynchronizerUsageSupported 1.0

# TYPE java_lang_Runtime_BootClassPathSupported untyped
java_lang_Runtime_BootClassPathSupported 1.0

# TYPE java_nio_BufferPool_Count untyped
java_nio_BufferPool_Count{name="direct",} 4.0
java_nio_BufferPool_Count{name="mapped",} 0.0

# TYPE java_lang_GarbageCollector_LastGcInfo_GcThreadCount untyped
java_lang_GarbageCollector_LastGcInfo_GcThreadCount{name="PS Scavenge",} 8.0
java_lang_GarbageCollector_LastGcInfo_GcThreadCount{name="PS MarkSweep",} 8.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS Scavenge",key="Compressed Class Space",} 8871936.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS Scavenge",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS Scavenge",key="PS Old Gen",} 4.17857536E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS Scavenge",key="Metaspace",} 4.6882816E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS Scavenge",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS Scavenge",key="Code Cache",} 1.1337728E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS MarkSweep",key="Compressed Class Space",} 6905856.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS MarkSweep",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS MarkSweep",key="PS Old Gen",} 2.45366784E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS MarkSweep",key="Metaspace",} 3.6265984E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS MarkSweep",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_committed{name="PS MarkSweep",key="Code Cache",} 8585216.0

# TYPE java_lang_Threading_CurrentThreadCpuTimeSupported untyped
java_lang_Threading_CurrentThreadCpuTimeSupported 1.0

# TYPE java_lang_ClassLoading_LoadedClassCount untyped
java_lang_ClassLoading_LoadedClassCount 10497.0

# TYPE java_lang_MemoryPool_CollectionUsage_init untyped
java_lang_MemoryPool_CollectionUsage_init{name="PS Old Gen",} 3.58088704E8
java_lang_MemoryPool_CollectionUsage_init{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_CollectionUsage_init{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_MemoryPool_PeakUsage_max untyped
java_lang_MemoryPool_PeakUsage_max{name="Metaspace",} -1.0
java_lang_MemoryPool_PeakUsage_max{name="PS Old Gen",} 5.726797824E9
java_lang_MemoryPool_PeakUsage_max{name="PS Eden Space",} 2.819096576E9
java_lang_MemoryPool_PeakUsage_max{name="Code Cache",} 2.5165824E8
java_lang_MemoryPool_PeakUsage_max{name="Compressed Class Space",} 1.073741824E9
java_lang_MemoryPool_PeakUsage_max{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_MemoryPool_Usage_max untyped
java_lang_MemoryPool_Usage_max{name="Metaspace",} -1.0
java_lang_MemoryPool_Usage_max{name="PS Old Gen",} 5.726797824E9
java_lang_MemoryPool_Usage_max{name="PS Eden Space",} 2.819096576E9
java_lang_MemoryPool_Usage_max{name="Code Cache",} 2.5165824E8
java_lang_MemoryPool_Usage_max{name="Compressed Class Space",} 1.073741824E9
java_lang_MemoryPool_Usage_max{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_GarbageCollector_Valid untyped
java_lang_GarbageCollector_Valid{name="PS Scavenge",} 1.0
java_lang_GarbageCollector_Valid{name="PS MarkSweep",} 1.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS Scavenge",key="Compressed Class Space",} 8621048.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS Scavenge",key="PS Survivor Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS Scavenge",key="PS Old Gen",} 2.3144792E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS Scavenge",key="Metaspace",} 4.6086976E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS Scavenge",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS Scavenge",key="Code Cache",} 1.126112E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS MarkSweep",key="Compressed Class Space",} 6590808.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS MarkSweep",key="PS Survivor Space",} 1.6433208E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS MarkSweep",key="PS Old Gen",} 1.5713664E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS MarkSweep",key="Metaspace",} 3.576556E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS MarkSweep",key="PS Eden Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_used{name="PS MarkSweep",key="Code Cache",} 8525504.0

# TYPE java_lang_Threading_ThreadAllocatedMemoryEnabled untyped
java_lang_Threading_ThreadAllocatedMemoryEnabled 1.0

# TYPE java_lang_MemoryManager_Valid untyped
java_lang_MemoryManager_Valid{name="CodeCacheManager",} 1.0
java_lang_MemoryManager_Valid{name="Metaspace Manager",} 1.0

# TYPE java_lang_MemoryPool_Usage_init untyped
java_lang_MemoryPool_Usage_init{name="Metaspace",} 0.0
java_lang_MemoryPool_Usage_init{name="PS Old Gen",} 3.58088704E8
java_lang_MemoryPool_Usage_init{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_Usage_init{name="Code Cache",} 2555904.0
java_lang_MemoryPool_Usage_init{name="Compressed Class Space",} 0.0
java_lang_MemoryPool_Usage_init{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_OperatingSystem_ProcessCpuLoad untyped
java_lang_OperatingSystem_ProcessCpuLoad 0.0

# TYPE java_lang_MemoryPool_CollectionUsage_committed untyped
java_lang_MemoryPool_CollectionUsage_committed{name="PS Old Gen",} 4.17857536E8
java_lang_MemoryPool_CollectionUsage_committed{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_CollectionUsage_committed{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_OperatingSystem_TotalPhysicalMemorySize untyped
java_lang_OperatingSystem_TotalPhysicalMemorySize 3.4359738368E10

# TYPE java_lang_Memory_NonHeapMemoryUsage_committed untyped
java_lang_Memory_NonHeapMemoryUsage_committed 7.7971456E7

# TYPE java_lang_Compilation_TotalCompilationTime untyped
java_lang_Compilation_TotalCompilationTime 4971.0

# TYPE java_lang_Memory_Verbose untyped
java_lang_Memory_Verbose 0.0

# TYPE java_lang_MemoryPool_Valid untyped
java_lang_MemoryPool_Valid{name="Metaspace",} 1.0
java_lang_MemoryPool_Valid{name="PS Old Gen",} 1.0
java_lang_MemoryPool_Valid{name="PS Eden Space",} 1.0
java_lang_MemoryPool_Valid{name="Code Cache",} 1.0
java_lang_MemoryPool_Valid{name="Compressed Class Space",} 1.0
java_lang_MemoryPool_Valid{name="PS Survivor Space",} 1.0

# TYPE java_lang_OperatingSystem_FreeSwapSpaceSize untyped
java_lang_OperatingSystem_FreeSwapSpaceSize 1.410596864E9

# TYPE java_lang_MemoryPool_UsageThresholdExceeded untyped
java_lang_MemoryPool_UsageThresholdExceeded{name="Metaspace",} 0.0
java_lang_MemoryPool_UsageThresholdExceeded{name="PS Old Gen",} 0.0
java_lang_MemoryPool_UsageThresholdExceeded{name="Code Cache",} 0.0
java_lang_MemoryPool_UsageThresholdExceeded{name="Compressed Class Space",} 0.0

# TYPE java_lang_Threading_CurrentThreadCpuTime untyped
java_lang_Threading_CurrentThreadCpuTime 8.008E7

# TYPE java_lang_MemoryPool_CollectionUsageThreshold untyped
java_lang_MemoryPool_CollectionUsageThreshold{name="PS Old Gen",} 0.0
java_lang_MemoryPool_CollectionUsageThreshold{name="PS Eden Space",} 0.0
java_lang_MemoryPool_CollectionUsageThreshold{name="PS Survivor Space",} 0.0

# TYPE java_lang_GarbageCollector_CollectionTime untyped
java_lang_GarbageCollector_CollectionTime{name="PS Scavenge",} 30.0
java_lang_GarbageCollector_CollectionTime{name="PS MarkSweep",} 56.0

# TYPE java_lang_Compilation_CompilationTimeMonitoringSupported untyped
java_lang_Compilation_CompilationTimeMonitoringSupported 1.0

# TYPE java_lang_MemoryPool_Usage_committed untyped
java_lang_MemoryPool_Usage_committed{name="Metaspace",} 5.4747136E7
java_lang_MemoryPool_Usage_committed{name="PS Old Gen",} 4.17857536E8
java_lang_MemoryPool_Usage_committed{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_Usage_committed{name="Code Cache",} 1.277952E7
java_lang_MemoryPool_Usage_committed{name="Compressed Class Space",} 1.04448E7
java_lang_MemoryPool_Usage_committed{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_Memory_NonHeapMemoryUsage_init untyped
java_lang_Memory_NonHeapMemoryUsage_init 2555904.0

# TYPE java_lang_MemoryPool_PeakUsage_init untyped
java_lang_MemoryPool_PeakUsage_init{name="Metaspace",} 0.0
java_lang_MemoryPool_PeakUsage_init{name="PS Old Gen",} 3.58088704E8
java_lang_MemoryPool_PeakUsage_init{name="PS Eden Space",} 1.34742016E8
java_lang_MemoryPool_PeakUsage_init{name="Code Cache",} 2555904.0
java_lang_MemoryPool_PeakUsage_init{name="Compressed Class Space",} 0.0
java_lang_MemoryPool_PeakUsage_init{name="PS Survivor Space",} 2.2020096E7

# TYPE java_lang_GarbageCollector_LastGcInfo_startTime untyped
java_lang_GarbageCollector_LastGcInfo_startTime{name="PS Scavenge",} 2876.0
java_lang_GarbageCollector_LastGcInfo_startTime{name="PS MarkSweep",} 2106.0

# TYPE java_lang_OperatingSystem_AvailableProcessors untyped
java_lang_OperatingSystem_AvailableProcessors 8.0

# TYPE java_lang_MemoryPool_CollectionUsageThresholdSupported untyped
java_lang_MemoryPool_CollectionUsageThresholdSupported{name="Metaspace",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdSupported{name="PS Old Gen",} 1.0
java_lang_MemoryPool_CollectionUsageThresholdSupported{name="PS Eden Space",} 1.0
java_lang_MemoryPool_CollectionUsageThresholdSupported{name="Code Cache",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdSupported{name="Compressed Class Space",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdSupported{name="PS Survivor Space",} 1.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS Scavenge",key="Compressed Class Space",} 1.073741824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS Scavenge",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS Scavenge",key="PS Old Gen",} 5.726797824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS Scavenge",key="Metaspace",} -1.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS Scavenge",key="PS Eden Space",} 2.819096576E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS Scavenge",key="Code Cache",} 2.5165824E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS MarkSweep",key="Compressed Class Space",} 1.073741824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS MarkSweep",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS MarkSweep",key="PS Old Gen",} 5.726797824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS MarkSweep",key="Metaspace",} -1.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS MarkSweep",key="PS Eden Space",} 2.819096576E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_max{name="PS MarkSweep",key="Code Cache",} 2.5165824E8

# TYPE java_lang_ClassLoading_UnloadedClassCount untyped
java_lang_ClassLoading_UnloadedClassCount 0.0

# TYPE java_nio_BufferPool_MemoryUsed untyped
java_nio_BufferPool_MemoryUsed{name="direct",} 73729.0
java_nio_BufferPool_MemoryUsed{name="mapped",} 0.0

# TYPE java_nio_BufferPool_TotalCapacity untyped
java_nio_BufferPool_TotalCapacity{name="direct",} 73728.0
java_nio_BufferPool_TotalCapacity{name="mapped",} 0.0

# TYPE java_lang_Memory_HeapMemoryUsage_used untyped
java_lang_Memory_HeapMemoryUsage_used 1.3891992E8

# TYPE java_lang_MemoryPool_CollectionUsage_used untyped
java_lang_MemoryPool_CollectionUsage_used{name="PS Old Gen",} 2.3144792E7
java_lang_MemoryPool_CollectionUsage_used{name="PS Eden Space",} 0.0
java_lang_MemoryPool_CollectionUsage_used{name="PS Survivor Space",} 1.8829768E7

# TYPE java_lang_Memory_HeapMemoryUsage_init untyped
java_lang_Memory_HeapMemoryUsage_init 5.36870912E8

# TYPE java_lang_OperatingSystem_SystemCpuLoad untyped
java_lang_OperatingSystem_SystemCpuLoad 0.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS Scavenge",key="Compressed Class Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS Scavenge",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS Scavenge",key="PS Old Gen",} 3.58088704E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS Scavenge",key="Metaspace",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS Scavenge",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS Scavenge",key="Code Cache",} 2555904.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS MarkSweep",key="Compressed Class Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS MarkSweep",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS MarkSweep",key="PS Old Gen",} 3.58088704E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS MarkSweep",key="Metaspace",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS MarkSweep",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_init{name="PS MarkSweep",key="Code Cache",} 2555904.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS Scavenge",key="Compressed Class Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS Scavenge",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS Scavenge",key="PS Old Gen",} 3.58088704E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS Scavenge",key="Metaspace",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS Scavenge",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS Scavenge",key="Code Cache",} 2555904.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS MarkSweep",key="Compressed Class Space",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS MarkSweep",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS MarkSweep",key="PS Old Gen",} 3.58088704E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS MarkSweep",key="Metaspace",} 0.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS MarkSweep",key="PS Eden Space",} 1.34742016E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageBeforeGc_init{name="PS MarkSweep",key="Code Cache",} 2555904.0

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

# TYPE java_lang_MemoryPool_CollectionUsageThresholdExceeded untyped
java_lang_MemoryPool_CollectionUsageThresholdExceeded{name="PS Old Gen",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdExceeded{name="PS Eden Space",} 0.0
java_lang_MemoryPool_CollectionUsageThresholdExceeded{name="PS Survivor Space",} 0.0

# TYPE java_lang_GarbageCollector_LastGcInfo_duration untyped
java_lang_GarbageCollector_LastGcInfo_duration{name="PS Scavenge",} 7.0
java_lang_GarbageCollector_LastGcInfo_duration{name="PS MarkSweep",} 29.0

# TYPE java_lang_Threading_ObjectMonitorUsageSupported untyped
java_lang_Threading_ObjectMonitorUsageSupported 1.0

# TYPE java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max untyped
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS Scavenge",key="Compressed Class Space",} 1.073741824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS Scavenge",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS Scavenge",key="PS Old Gen",} 5.726797824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS Scavenge",key="Metaspace",} -1.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS Scavenge",key="PS Eden Space",} 2.819096576E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS Scavenge",key="Code Cache",} 2.5165824E8
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS MarkSweep",key="Compressed Class Space",} 1.073741824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS MarkSweep",key="PS Survivor Space",} 2.2020096E7
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS MarkSweep",key="PS Old Gen",} 5.726797824E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS MarkSweep",key="Metaspace",} -1.0
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS MarkSweep",key="PS Eden Space",} 2.819096576E9
java_lang_GarbageCollector_LastGcInfo_memoryUsageAfterGc_max{name="PS MarkSweep",key="Code Cache",} 2.5165824E8

# TYPE java_lang_MemoryPool_UsageThresholdCount untyped
java_lang_MemoryPool_UsageThresholdCount{name="Metaspace",} 0.0
java_lang_MemoryPool_UsageThresholdCount{name="PS Old Gen",} 0.0
java_lang_MemoryPool_UsageThresholdCount{name="Code Cache",} 0.0
java_lang_MemoryPool_UsageThresholdCount{name="Compressed Class Space",} 0.0

# TYPE java_lang_Threading_ThreadCpuTimeEnabled untyped
java_lang_Threading_ThreadCpuTimeEnabled 1.0

# TYPE java_lang_OperatingSystem_OpenFileDescriptorCount untyped
java_lang_OperatingSystem_OpenFileDescriptorCount 174.0

# TYPE jmx_scrape_duration_seconds gauge
jmx_scrape_duration_seconds 10.110807166

# TYPE jmx_scrape_error gauge
jmx_scrape_error 0.0

# TYPE jvm_gc_collection_seconds summary
jvm_gc_collection_seconds_count{gc="PS Scavenge",} 3.0
jvm_gc_collection_seconds_sum{gc="PS Scavenge",} 0.03
jvm_gc_collection_seconds_count{gc="PS MarkSweep",} 2.0
jvm_gc_collection_seconds_sum{gc="PS MarkSweep",} 0.056

# TYPE jvm_memory_bytes_used gauge
jvm_memory_bytes_used{area="heap",} 1.4161508E8
jvm_memory_bytes_used{area="nonheap",} 7.5698048E7

# TYPE jvm_memory_bytes_committed gauge
jvm_memory_bytes_committed{area="heap",} 5.74619648E8
jvm_memory_bytes_committed{area="nonheap",} 7.82336E7

# TYPE jvm_memory_bytes_max gauge
jvm_memory_bytes_max{area="heap",} 7.635730432E9
jvm_memory_bytes_max{area="nonheap",} -1.0

# TYPE jvm_memory_bytes_init gauge
jvm_memory_bytes_init{area="heap",} 5.36870912E8
jvm_memory_bytes_init{area="nonheap",} 2555904.0

# TYPE jvm_memory_pool_bytes_used gauge
jvm_memory_pool_bytes_used{pool="Code Cache",} 1.1661568E7
jvm_memory_pool_bytes_used{pool="Metaspace",} 5.3987176E7
jvm_memory_pool_bytes_used{pool="Compressed Class Space",} 1.0049616E7
jvm_memory_pool_bytes_used{pool="PS Eden Space",} 9.9632328E7
jvm_memory_pool_bytes_used{pool="PS Survivor Space",} 1.8829768E7
jvm_memory_pool_bytes_used{pool="PS Old Gen",} 2.3152984E7

# TYPE jvm_memory_pool_bytes_committed gauge
jvm_memory_pool_bytes_committed{pool="Code Cache",} 1.277952E7
jvm_memory_pool_bytes_committed{pool="Metaspace",} 5.500928E7
jvm_memory_pool_bytes_committed{pool="Compressed Class Space",} 1.04448E7
jvm_memory_pool_bytes_committed{pool="PS Eden Space",} 1.34742016E8
jvm_memory_pool_bytes_committed{pool="PS Survivor Space",} 2.2020096E7
jvm_memory_pool_bytes_committed{pool="PS Old Gen",} 4.17857536E8

# TYPE jvm_memory_pool_bytes_max gauge
jvm_memory_pool_bytes_max{pool="Code Cache",} 2.5165824E8
jvm_memory_pool_bytes_max{pool="Metaspace",} -1.0
jvm_memory_pool_bytes_max{pool="Compressed Class Space",} 1.073741824E9
jvm_memory_pool_bytes_max{pool="PS Eden Space",} 2.819096576E9
jvm_memory_pool_bytes_max{pool="PS Survivor Space",} 2.2020096E7
jvm_memory_pool_bytes_max{pool="PS Old Gen",} 5.726797824E9

# TYPE jvm_memory_pool_bytes_init gauge
jvm_memory_pool_bytes_init{pool="Code Cache",} 2555904.0
jvm_memory_pool_bytes_init{pool="Metaspace",} 0.0
jvm_memory_pool_bytes_init{pool="Compressed Class Space",} 0.0
jvm_memory_pool_bytes_init{pool="PS Eden Space",} 1.34742016E8
jvm_memory_pool_bytes_init{pool="PS Survivor Space",} 2.2020096E7
jvm_memory_pool_bytes_init{pool="PS Old Gen",} 3.58088704E8

# TYPE jvm_info gauge
jvm_info{version="1.8.0_191-b12",vendor="Oracle Corporation",runtime="Java(TM) SE Runtime Environment",} 1.0

# TYPE jvm_memory_pool_allocated_bytes_total counter
jvm_memory_pool_allocated_bytes_total{pool="Code Cache",} 1.126112E7
jvm_memory_pool_allocated_bytes_total{pool="PS Eden Space",} 3.60454968E8
jvm_memory_pool_allocated_bytes_total{pool="PS Old Gen",} 2.3152984E7
jvm_memory_pool_allocated_bytes_total{pool="PS Survivor Space",} 5.1496368E7
jvm_memory_pool_allocated_bytes_total{pool="Compressed Class Space",} 8621048.0
jvm_memory_pool_allocated_bytes_total{pool="Metaspace",} 4.6086976E7

Naming of metrics is determined via:

- `jmx _ db_name _ column_name`

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
