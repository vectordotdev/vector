# Configuration Specification

This document specifies Vector's configuration for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

- [Introduction](#introduction)
- [Scope](#scope)
- [Terminology](#terminology)
  - [Flag](#flag)
  - [Entity](#entity)
  - [Option](#option)
- [Schema](#schema)
  - [Naming](#naming)
    - [Flag naming](#flag-naming)
    - [Entity naming](#entity-naming)
    - [Option naming](#option-naming)
  - [Types](#types)
  - [Polymorphism](#polymorphism)
    - [Entity polymorphism](#entity-polymorphism)
    - [Option polymorphism](#option-polymorphism)

## Introduction

Vector's runtime behavior is expressed through user-defined configuration files
intended to be written directly by users. Therefore, the quality of Vector's
configuration largely affects Vector's user experience. This document aims to
make Vector's configuration as high quality as possible in order to achieve the
[best in class user experience][user_experience].

## Scope

This specification is focused on broad configuration guidelines and not specific
options. It is intended to guide Vector's configuration without tedious
management. When necessary, specific configuration will be covered in other
relevant specifications, such as the [component specification].

## Terminology

### Flag

"Flag" refers to a CLI flag provided when running Vector.

### Entity

"Entity" refers to a Vector concept used to model Vector's processing graph.
Sources, transforms, sinks, and enrichment tables are all examples of entities.
Entities are defined under a root-level type followed by a user-defined ID
containing the entity's options.

### Option

"Option" refers to a leaf field that takes a primitive value. Options are nested
under entities and also used to define global Vector behavior.

## Schema

### Naming

#### Flag naming

- MUST only contain ASCII alphanumeric, lowercase, and hyphens
- MUST be in kebab-case format when multiple words are used (e.g., `config-dir`)
- For flags that take a value, but are also able to be "disabled", they SHOULD NOT use a sentinel
  value. Instead they SHOULD have a second flag added prefixed with `no-` and SHOULD leave off any
  unit suffixes. For example, to disable `--graceful-shutdown-limit-secs`,
  a `--no-graceful-shutdown` flag was added. Vector MUST NOT allow both the flag and its negative to
  be specified at the same time.

#### Entity naming

- MUST only contain ASCII alphanumeric, lowercase, and underscores
  - The `.` character is reserved for special purposes (e.g., error stream routing)
- MUST be in snake case format when multiple words are used (e.g., `timeout_seconds`)

#### Option naming

- MUST only contain ASCII alphanumeric, lowercase, and underscores
- MUST be in snake case format when multiple words are used (e.g., `timeout_seconds`)
- SHOULD use nouns, not verbs, as names (e.g., `fingerprint` instead of `fingerprinting`)
- MUST suffix options with their _full_ unit name (e.g., `_megabytes` rather than `_mb`) or the
  following abbreviations for time units: `_secs`, `_ms`, `_ns`.
- SHOULD consistent with units within the same scope. (e.g., don't mix seconds and milliseconds)
- MUST NOT repeat the name space in the option name (e.g., `fingerprint.bytes` instead of `fingerprint.fingerprint_bytes`)

### Types

Types MUST consist of [JSON types] only, minus the `null` type:

- `string`
- `number`
- `integer`
- `object`
- `array`
- `boolean`

### Polymorphism

#### Entity polymorphism

By nature entities being namespaced by user-defined IDs, polymorphism MUST be
supported for entity namespaces.

#### Option polymorphism

Options MUST NOT support polymorphism:

- MUST be strongly typed
- MUST be [externally tagged] for mutually exclusive sets of options
  - REQUIRED to implement a top-level `type` key that accept the tag value

For example:

```toml
buffer.type = "memory"
buffer.memory.max_events = 10_000
```

The above configures a Vector memory buffer which can be switched to disk as
well:

```toml
buffer.type = "disk"
buffer.disk.max_bytes = 1_000_000_000
```

[component specification]: component.md
[external tagging]: https://docs.rs/serde_tagged/0.2.0/serde_tagged/ser/external/index.html
[json types]: http://json-schema.org/understanding-json-schema/reference/type.html
[user_experience]: ../USER_EXPERIENCE_DESIGN.md
