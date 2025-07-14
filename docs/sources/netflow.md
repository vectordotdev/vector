# NetFlow Source

The NetFlow source collects network flow data from NetFlow/IPFIX/sFlow exporters. It supports multiple flow protocols and can handle template-based protocols like NetFlow v9 and IPFIX.

## Configuration

```yaml
sources:
  netflow:
    type: netflow
    address: "0.0.0.0:2055"  # Default NetFlow port
    max_length: 65536  # Maximum packet size
    protocols:
      - "netflow_v5"
      - "netflow_v9" 
      - "ipfix"
      - "sflow"
    include_raw_data: false  # Set to true for debugging
    max_templates: 1000  # Maximum templates to cache per observation domain
    template_timeout_secs: 3600  # Template cache timeout (1 hour)
    multicast_groups:  # Optional: join multicast groups
      - "224.0.0.2"
      - "224.0.0.4"
    receive_buffer_bytes: 262144  # Optional: set socket receive buffer
```

## Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `address` | `string` | `"0.0.0.0:2055"` | The address to bind to for receiving NetFlow packets |
| `max_length` | `integer` | `65536` | Maximum size of incoming packets |
| `protocols` | `array` | `["netflow_v5", "netflow_v9", "ipfix", "sflow"]` | List of supported flow protocols |
| `include_raw_data` | `boolean` | `false` | Whether to include raw packet data in events for debugging |
| `max_templates` | `integer` | `1000` | Maximum number of templates to cache per observation domain |
| `template_timeout_secs` | `integer` | `3600` | Template cache timeout in seconds |
| `multicast_groups` | `array` | `[]` | List of IPv4 multicast groups to join |
| `receive_buffer_bytes` | `integer` | `null` | Size of the receive buffer for the listening socket |

## Supported Protocols

### NetFlow v5
- Fixed format flow records
- No templates required
- Most common NetFlow version

### NetFlow v9
- Template-based format
- Supports variable-length fields
- Templates are cached automatically

### IPFIX
- Modern standard (RFC 7011)
- Template-based format
- Supports enterprise-specific fields
- Templates are cached automatically

### sFlow
- Sampled flow data
- Different header format
- Includes counter samples

## Output Events

The source generates log events with the following structure:

### NetFlow v5 Events
```json
{
  "flow_type": "netflow_v5",
  "version": 5,
  "sys_uptime": 123456,
  "unix_secs": 1640995200,
  "flow_sequence": 1,
  "engine_type": 0,
  "engine_id": 0,
  "sampling_interval": 1000,
  "src_addr": "192.168.1.1",
  "dst_addr": "10.0.0.1",
  "src_port": 80,
  "dst_port": 443,
  "protocol": 6,
  "protocol_name": "TCP",
  "packets": 100,
  "octets": 1024,
  "tcp_flags": 2,
  "tos": 0,
  "src_as": 65000,
  "dst_as": 65001,
  "input": 1,
  "output": 2,
  "first": 1000,
  "last": 2000,
  "flow_duration_ms": 1000
}
```

### NetFlow v9/IPFIX Events
```json
{
  "flow_type": "netflow_v9_data",
  "template_id": 256,
  "source_id": 1,
  "in_bytes": 1024,
  "in_packets": 100,
  "ipv4_src_addr": "192.168.1.1",
  "ipv4_dst_addr": "10.0.0.1",
  "l4_src_port": 80,
  "l4_dst_port": 443,
  "protocol": 6,
  "tcp_flags": 2
}
```

### sFlow Events
```json
{
  "flow_type": "sflow",
  "version": 5,
  "agent_address": "192.168.1.1",
  "sub_agent_id": 0,
  "sequence_number": 1,
  "sys_uptime": 123456,
  "num_samples": 1
}
```

## Template Caching

For template-based protocols (NetFlow v9 and IPFIX), the source automatically caches templates received from exporters. Templates are keyed by:

- Exporter address (peer_addr)
- Observation domain ID
- Template ID

Templates are automatically cleaned up after the configured timeout period.

## Multicast Support

The source can join multicast groups to receive NetFlow traffic from multiple sources. When using multicast:

1. Set the listening address to `0.0.0.0` (not a specific interface)
2. Configure the `multicast_groups` option with the desired multicast addresses
3. Ensure your network infrastructure supports the multicast groups

## Performance Considerations

- **Template Cache Size**: Monitor memory usage if you have many exporters with different templates
- **Packet Size**: Adjust `max_length` based on your network's MTU
- **Receive Buffer**: Increase `receive_buffer_bytes` if you experience packet drops
- **Protocol Filtering**: Only enable the protocols you need to reduce processing overhead

## Troubleshooting

### Enable Raw Data
Set `include_raw_data: true` to include base64-encoded raw packet data in events for debugging.

### Check Template Cache
Monitor template cache size and cleanup frequency. If templates are expiring too quickly, increase `template_timeout_secs`.

### Multicast Issues
- Ensure the listening address is `0.0.0.0`
- Check that multicast groups are valid IPv4 addresses
- Verify network infrastructure supports the multicast groups

### Packet Drops
- Increase `receive_buffer_bytes`
- Check system UDP buffer limits
- Monitor network interface statistics

## Examples

### Basic NetFlow v5 Collection
```yaml
sources:
  netflow:
    type: netflow
    address: "0.0.0.0:2055"
    protocols:
      - "netflow_v5"
```

### IPFIX with Template Caching
```yaml
sources:
  ipfix:
    type: netflow
    address: "0.0.0.0:4739"
    protocols:
      - "ipfix"
    max_templates: 2000
    template_timeout_secs: 7200
```

### Multicast NetFlow Collection
```yaml
sources:
  netflow_multicast:
    type: netflow
    address: "0.0.0.0:2055"
    multicast_groups:
      - "224.0.0.2"
      - "224.0.0.4"
    protocols:
      - "netflow_v5"
      - "netflow_v9"
```

### Debug Configuration
```yaml
sources:
  netflow_debug:
    type: netflow
    address: "0.0.0.0:2055"
    protocols:
      - "netflow_v5"
      - "netflow_v9"
      - "ipfix"
      - "sflow"
    include_raw_data: true
    max_templates: 100
    template_timeout_secs: 1800
``` 