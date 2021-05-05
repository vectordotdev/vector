---
title: GeoIP
kind: transform
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### State

{{< snippet "stateless" >}}

### Supported MaxMind databases

The `geoip` transform currently supports the following [MaxMind] databases:

Database | Free/paid | Description
:--------|:----------|:-----------
[GeoLite2-ASN.mmdb][asn] | free | Determine the autonomous system number and organization associated with an IP address.
[GeoLite2-City.mmdb][city] | free | Determine the country, subdivisions, city, and postal code associated with IPv4 and IPv6 addresses worldwide.
[GeoIP2-City.mmdb][ip2_city] | paid | Determine the country, subdivisions, city, and postal code associated with IPv4 and IPv6 addresses worldwide.
[GeoIP2-ISP.mmdb][isp] paid | Determine the Internet Service Provider (ISP), organization name, and autonomous system organization and number associated with an IP address.

The database files should be in the [MaxMind DB file format][file_format].

[asn]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
[city]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
[file_format]: https://maxmind.github.io/MaxMind-DB
[ip2_city]: https://www.maxmind.com/en/geoip2-city
[isp]: https://www.maxmind.com/en/geoip2-isp-database
[maxmind]: https://www.maxmind.com/en/home
