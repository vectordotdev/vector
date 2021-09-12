# Configuration Specification

This document specifies Vector's configuration for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Introduction](#introduction)
1. [Scope](#scope)
1. [Terminology](#terminology)
   1. [Entity](#entity)
   1. [Option](#option)
1. [Schema](#schema)
   1. [Naming](#naming)
      1. [Entity naming](#entity-naming)
      1. [Option naming](#option-naming)
   1. [Types](#types)
   1. [Polymorphism](#polymorphism)
      1. [Entity polymorphism](#entity-polymorphism)
      1. [Option polymorphism](#option-polymorphism)

<!-- /MarkdownTOC -->

## Introduction

Vector's runtime behavior is expressed through user-defined configuration files
intended to be written directly by users. Therefore, the quality of Vector's
configuration largely affects Vector's user experience. This document aims to
make Vector's configuration as high quality as possible in order to achieve a
[best in class user experience][user_experience].

## Scope

This specification is focused on broad configuration guidelines and not specific
options. It is intended to guide Vector's configuration without tedious
management. When necessary, specific configuration will be covered in other
relevant specifications, such as the [component specifiction].

## Terminology

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

#### Entity naming

* MUST only contain ASCII alphanumeric, lowercase, and underscores
  * The `.` character is reserved for special purposes (e.g., error stream routing)
* MUST be in snake case format when multiple words are used (e.g., `timeout_seconds`)

#### Option naming

* MUST only contain ASCII alphanumeric, lowercase, and underscores
* MUST be in snake case format when multiple words are used (e.g., `timeout_seconds`)
* SHOULD use nouns, not verbs, as names (e.g., `fingerprint` instead of `fingerprinting`)
* MUST suffix options with their _full_ unit name (e.g., `_seconds`, `_bytes`, etc.)
* SHOULD consistent with units within the same scope. (e.g., don't mix seconds and milliseconds)
* MUST NOT repeat the name space in the option name (e.g., `fingerprint.bytes` instead of `fingerprint.fingerprint_bytes`)

### Types

Types MUST consist of [JSON types] only:

* `string`
* `number`
* `integer`
* `object`
* `array`
* `boolean`
* `null`

### Polymorphism

#### Entity polymorphism

By nature entities being namespaced by user-defined IDs, polymorphism MUST be
supported for entity namespaces.

#### Option polymorphism

Options MUST NOT support polymorphism:

* MUST be strongly typed
* MUST be [externally tagged] for mutually exclusive sets of options
  * REQUIRED to implement a top-level `type` key that accept the tag value

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
[JSON types]: http://json-schema.org/understanding-json-schema/reference/type.html
[user_experience]: ../USER_EXPERIENCE_DESIGN.md
