---
title: NGINX metrics
description: Collect metrics from [NGINX](https://nginx.com)
kind: source
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

### Context

{{< snippet "context" >}}

### Module `ngx_http_stub_status_module`

The [`ngx_http_stub_status_module`][ngx_http_stub_status_module] module provides access to basic status information. Basic status information is a simple web page with text data.

### State

{{< snippet "stateless" >}}

[ngx_http_stub_status_module]: http://nginx.org/en/docs/http/ngx_http_stub_status_module.html
