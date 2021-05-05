---
title: MongoDB metrics
description: Collect metrics from the [MongoDB](https://mongodb.com) database
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

### MongoDB `serverStatus` command

The [`serverStatus`][server_status] command returns a document that provides an overview of the database's state. The output fields vary depending on the version of MongoDB, underlying operating system platform, the storage engine, and the kind of node, including `mongos`, [`mongod`][mongod] or `replica set` member.

### State

{{< snippet "stateless" >}}

[mongod]: https://vector.dev/docs/reference/configuration/sources/mongodb_metrics/#mongod
[server_status]: https://docs.mongodb.com/manual/reference/command/serverStatus
