# NetFlow Source

The NetFlow source collects network flow data from NetFlow/IPFIX/sFlow exporters. It supports multiple flow protocols and can handle template-based protocols like NetFlow v9 and IPFIX with enterprise-specific field support.

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
    drop_events_without_templates: false  # Drop events when no template is available
    max_field_length: 1024  # Maximum length for field values
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
| `drop_events_without_templates` | `boolean` | `false` | Whether to drop events when no template is available |
| `max_field_length` | `integer` | `1024` | Maximum length for field values to prevent memory issues |
| `max_templates` | `integer` | `1000` | Maximum number of templates to cache per observation domain |
| `template_timeout_secs` | `integer` | `3600` | Template cache timeout in seconds |
| `multicast_groups` | `array` | `[]` | List of IPv4 multicast groups to join |
| `receive_buffer_bytes` | `integer` | `null` | Size of the receive buffer for the listening socket |

## Supported Protocols

### NetFlow v5
- Fixed format flow records (48 bytes per record)
- No templates required
- Most common NetFlow version
- Includes source/destination addresses, ports, protocol, packet/byte counts

### NetFlow v9
- Template-based format with variable-length fields
- Templates are cached automatically
- Supports enterprise-specific fields
- More flexible than v5 but requires template management

### IPFIX (Internet Protocol Flow Information Export)
- Modern standard (RFC 7011)
- Template-based format with variable-length fields
- Supports enterprise-specific fields (e.g., HPE Aruba)
- RFC-compliant set IDs (2=Template, 3=Options Template, 256+=Data)
- Variable-length field support (length=65535)

### sFlow (Sampled Flow)
- Sampled flow data with different record types
- Supports flow samples, counter samples, and expanded variants
- Includes agent address and sampling information

## Enterprise Field Support

The NetFlow source supports enterprise-specific fields through the field parser registry:

### HPE Aruba Enterprise (ID: 23867)
- Client and server IPv4 addresses
- Connection statistics and delays
- Application information and zones
- Transaction duration metrics

### Custom Enterprise Support
You can extend the source with custom enterprise field parsers by implementing the `FieldParser` trait.

## Template Management

For template-based protocols (NetFlow v9, IPFIX):

- **Automatic Caching**: Templates are automatically cached per observation domain
- **Timeout Management**: Templates expire after configurable timeout
- **Memory Management**: Configurable maximum templates per domain
- **Validation**: Templates are validated before caching

## Variable-Length Fields

IPFIX and NetFlow v9 support variable-length fields:

- **1-byte length**: For fields ≤ 254 bytes
- **3-byte length**: For fields > 254 bytes (length=255 + 2-byte actual length)
- **Automatic parsing**: Handled transparently by the protocol parsers

## Error Handling

The source provides comprehensive error handling:

- **Parse Errors**: Invalid packets are logged with details
- **Template Errors**: Missing or invalid templates are handled gracefully
- **Field Errors**: Unparseable fields are logged with raw data (if enabled)
- **Memory Protection**: Field length limits prevent memory issues

## Performance Considerations

- **Template Caching**: Reduces parsing overhead for repeated templates
- **Field Length Limits**: Prevents memory issues with large fields
- **Event Filtering**: Option to drop events without templates
- **Buffer Management**: Configurable receive buffer sizes

## Examples

### Basic Configuration
```yaml
sources:
  netflow:
    type: netflow
    address: "0.0.0.0:2055"
    protocols: ["netflow_v5", "ipfix"]
```

### Production Configuration
```yaml
sources:
  netflow_prod:
    type: netflow
    address: "0.0.0.0:9995"
    protocols: ["ipfix"]
    include_raw_data: false
    drop_events_without_templates: true
    max_field_length: 512
    max_templates: 100
    template_timeout_secs: 1800
```

### Debug Configuration
```yaml
sources:
  netflow_debug:
    type: netflow
    address: "0.0.0.0:2055"
    protocols: ["netflow_v5", "netflow_v9", "ipfix", "sflow"]
    include_raw_data: true
    drop_events_without_templates: false
    max_field_length: 2048
```

## Troubleshooting

### Common Issues

1. **No events received**: Check firewall settings and multicast group configuration
2. **Memory usage high**: Reduce `max_templates` and `max_field_length`
3. **Parse errors**: Enable `include_raw_data` to see packet contents
4. **Missing templates**: Set `drop_events_without_templates: false` to see raw data

### Debug Mode

Enable debug mode to see detailed parsing information:

```yaml
sources:
  netflow:
    type: netflow
    address: "0.0.0.0:2055"
    include_raw_data: true
    drop_events_without_templates: false
```

This will include raw packet data and template information in events for debugging. 