---
title: Syslog
kind: source
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Context

{{< snippet "context" >}}

### Line delimiters

{{< snippet "line-delimiters" >}}

### Parsing

Vector makes a *best effort* to parse the various Syslog formats out in the wild. This includes [RFC 6587][rfc_6587], [RFC 5424][rfc_5424], [RFC 3164][rfc_3164], and other common variations (such as the Nginx Syslog style). It's unfortunate that the Syslog specification is not more accurately followed, but we hope Vector insulates you from these deviations.

If parsing fails, Vector will include the entire Syslog line in the [`message`](#message) key. If you find this happening often, we recommend using the [`socket` source][socket] combined with the [`regex_parser` transform][regex_parser] to implement your own ingestion and parsing scheme. Or [open an issue][issue] to request support for your specific format.

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[issue]: https://github.com/timberio/vector/issues/new?labels=type%3A+new+feature
[regex_parser]: /docs/reference/configuration/transforms/regex_parser
[rfc_3164]: https://tools.ietf.org/html/rfc3164
[rfc_5424]: https://tools.ietf.org/html/rfc5424
[rfc_6587]: https://tools.ietf.org/html/rfc6587
[socket]: /docs/reference/configuration/sources/socket
