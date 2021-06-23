---
# The main page at https://vector.dev
title: Vector

# What is Vector (with animated diagram)
what:
  title: Take control of your observability data
  description: Collect, transform, and route *all* your logs and metrics with *one* simple tool.

# Items listed in the "Why Vector?" section. Markdown is supported in the descriptions.
why:
- title: "Ultra fast and reliable"
  description: "Built in [Rust](https://rust-lang.org), Vector is blistering fast, memory efficient, and designed to handle the most demanding environments."
  icon: "clock.svg"
- title: "End to end"
  description: "Vector strives to be the only tool you need to get observability data from A to B, [deploying](/docs/setup/deployment) as an [daemon](/docs/setup/deployment/roles/#daemon)), [sidecar](/docs/setup/deployment/roles/#sidecar), or [aggregator](/docs/setup/deployment/roles/#aggregator)."
  icon: "chart.svg"
-  title: "Unified"
   description: "Vector supports [logs](/docs/about/under-the-hood/architecture/data-model/log) and [metrics](/docs/about/under-the-hood/architecture/data-model/metric), making it easy to collect and process all your observability data."
   icon: "hex.svg"
- title: "Vendor neutral"
  description: "Vector doesn't favor any storage and fosters a fair, open ecosystem with your best interest in mind. Lock-in free and future proof."
  icon: "lock.svg"
- title: "Programmable transforms"
  description: "[Programmable transforms](/docs/reference/configuration/transforms) give you the full power of programmable runtimes. Handle complex use cases without limitation."
  icon: "code.svg"
- title: "Clear guarantees"
  description: "Guarantees matter, and Vector is [clear on its guarantees](/docs/about/under-the-hood/guarantees), helping you to make the appropriate trade offs for your use case."
  icon: "laptop.svg"

# Platform section
platform:
  title: A complete, end-to-end platform.
  description: |
    Deploy Vector in a variety of roles to suit your use case.
    <br />
    Get data from point A to point B without patching tools together.
  # Selectable tabs with associated SVGs
  tabs:
  - Distributed
  - Centralized
  - Stream based

# Configuration section
configure:
  title: "Easy to configure"
  description: "A simple, composable format enables you to build flexible pipelines"
  filename: "/etc/vector/vector.toml"
  below: "Configuration examples are in [TOML](https://toml.io) but Vector also supports [YAML](https://yaml.org) and [JSON](https://json.org)"
  example_configs:
  - title: Parse NGINX logs
    config: |
      [sources.nginx_error_log]
      type = "file"
      include = ["/var/log/nginx*.log"]
      start_at_beginning = false
      ignore_older = 86400

      [transforms.nginx_error_parser]
      inputs = ["nginx_error_log"]
      type = "remap"
      source = '''
      # TODO
      '''
  - title: Something else
    config: |
      [sources.do_something_else]
      type = "other"

      [sinks.out]
      inputs = ["in"]
      type = "console"
      encoding.codec = "text"
  - title: Other thing entirely
    config: |
      [sources.do_other_thing_entirely]
      type = "other"

      [sinks.out]
      inputs = ["in"]
      type = "console"
      encoding.codec = "text"
  - title: And yet another
    config: |
      [sources.and_yet_another]
      type = "other"

      [sinks.out]
      inputs = ["in"]
      type = "console"
      encoding.codec = "text"

# Installation section
installation:
  title: Installs everywhere
  description: Packaged as a single binary. No dependencies, no runtime, and memory safe.
  logos:
  - logo: "kubernetes.svg"
    url: "/docs/setup/installation/platforms/kubernetes"
  - logo: "docker.svg"
    url: "/docs/setup/installation/platforms/docker"
  - logo: "linux.svg"
    url: "/docs/setup/installation/operating-systems"
  - logo: "raspbian.svg"
    url: "/docs/setup/installation/operating-systems/raspbian"
  - logo: "windows.svg"
    url: "/docs/setup/installation/operating-systems/windows"
  - logo: "apple.svg"
    url: "/docs/setup/installation/operating-systems/macos"
  features:
  - title: "Single binary"
    ionicon: "cube-outline"
  - title: "X86_64, ARM64/v7"
    ionicon: "hardware-chip-outline"
  - title: "No runtime"
    ionicon: "flash-outline"
  - title: "Memory safe"
    ionicon: "shield-outline"
  methods:
  - title: "Platforms"
    url: "/docs/setup/installation/platforms"
  - title: "Package managers"
    url: "/docs/setup/installation/package-managers"
  - title: "Operating systems"
    url: "/docs/setup/installation/operating-systems"
  - title: "Manual"
    url: "/docs/setup/installation/manual"

# Component cloud
components:
  title: Highly flexible processing topologies
  description: A wide range of sources, transforms, and sinks to choose from

# Community section
community:
  title: Backed by a strong open source community
  stats:
  - title: "GitHub stars"
    figure: "7.4k+"
  - title: "Contributors"
    figure: "150"
  - title: "Downloads"
    figure: "30m+"
  - title: "Countries"
    figure: "40"
---
