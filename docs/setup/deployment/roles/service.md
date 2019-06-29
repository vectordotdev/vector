---
description: Deploying and running Vector as a service
---

# Service Role

![](../../../assets/centralized-service.svg)

When Vector serves as a service, its purpose is to efficiently receive,
aggregate, and route data downstream. In this scenario, Vector is the primary
service on the host and should take full advantage of all resources.

## Vector Configuration

### Receiving Data

When Vector is deployed as a service it receives data over the network from
upstream clients or services. Relevant sources include the
[`vector`][docs.vector_source], [`syslog`][docs.syslog_source], and
[`tcp`][docs.tcp_source] sources.

### Performance Tuning

Vector is designed, by default, to [take full advantage of all system \
resources][docs.performance], which is usually preferred in the service role.
As a result, there is nothing special you need to do to improve performance.

### On-Disk Buffering

To ensure Vector does not lose data between restarts you'll need to switch
the buffer to use the disk for all relevant sinks. This can be accomplished
by adding a simple `[buffer]` table to each of your configured sinks. In
addition, we recommend specifying an explicit `data_dir` for Vector's buffer.
For example:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```c
data_dir = "/var/lib/vector"

[sinks.backups]
    type = "s3"
    # ...
    
    [sinks.backups.buffer]
        type = "disk"
        max_size = 5000000000 # 5gb
```
{% endcode-tabs-item %}
{% endcode-tabs %}

{% hint style="warning" %}
Please make sure that the Vector user has write access to the specified
`data_dir`.
{% endhint %}

Please note that there is a performance hit to enabling on-disk buffers of
about 3X. We believe this to be a worthwhile tradeoff to ensure data is not
lost across restarts.

## System Configuration

By default Vector is tuned for performance, there are no extra system level
configuration steps necessary to improve performance.

## Deployment

### Hardware

The hardware needed is highly dependent on your configuration and data volume.
Typically, Vector is CPU bound and not memory bound, especially if all buffers
are [configured to use the disk][docs.service_role.on-disk-buffering]. Our
[benchmarks][docs.performance] should give you a general idea of resource usage
in relation to specific pipelines and data volume.

#### CPU

Vector benefits greatly from parallel processing, the more cores the better.
For example, if you're on AWS, the `c5d.*` instances will give you the most
bang for your buck given their optimization towards CPU and the fact that
they include a fast NVME drive for on-disk buffers.

#### Memory

If you've configured [on-disk buffers][docs.service_role.on-disk-bufering],
then memory should not be your bottleneck. If you opted to keep buffers
in-memory, then you'll want to make sure you have at least 2X your cumulative
buffer size. For example, if you have an `elasticsearch` and `s3` sink
configured to use 100mb and 1gb, then you should ensure you have at least
2.2gb \(1.1 \* 2\) of memory available.

#### Disk

If you've configured on-disk buffers, then we recommend using local NVMe SSD
drives when possible. This will ensure disk IO does not become your bottleneck.
For example, if you're on AWS you'll want to choose an instance that includes a
local NVME SSD drive, such as the `c5d.*` instances. The size of the disk should
be at least 3 times your cumulative buffer size.

### Load balancing

TODO: make this better

If you've configured Vector to receive data over the network then you'll
benefit from load balancing. Select sinks offer built-in load balancing,
such as the [`http`][docs.http_sink], [`tcp`][docs.tcp_sink], and
[`vector`][docs.vector_sink] sinks. This is a very rudimentary form of load
balancing that requires all clients to know about the available downstream
hosts. A more formal load balancing strategy is outside of the scope of this
document, but is typically achieved by services such as
[AWS' ELB][url.aws_elb], [Haproxy][url.haproxy], [Nginx][url.nginx], and more.

## Administration

### Configuration Changes

Vector can be [reloaded][docs.reloading] to apply configuration changes.
This is the recommended strategy and should be used over restarting when
possible.

### Updating Vector

To [update][docs.updating] Vector you'll need to restart the process. Like any
service, restarting without disruption is achieved by higher level design
decisions, such as [load balancing][docs.service_role.load-balancing].


[docs.http_sink]: ../../../usage/configuration/sinks/http.md
[docs.performance]: ../../..docs/performance.md
[docs.reloading]: ../../..docs/usage/administration/reloading.md
[docs.syslog_source]: ../../../usage/configuration/sources/syslog.md
[docs.tcp_sink]: ../../../usage/configuration/sinks/tcp.md
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.updating]: ../../..docs/usage/administration/updating.md
[docs.vector_sink]: ../../../usage/configuration/sinks/vector.md
[docs.vector_source]: ../../../usage/configuration/sources/vector.md
[url.aws_elb]: https://aws.amazon.com/elasticloadbalancing/
[url.haproxy]: https://www.haproxy.org/
[url.nginx]: https://www.nginx.com/
