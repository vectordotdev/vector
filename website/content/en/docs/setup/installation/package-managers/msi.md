---
title: Install Vector using MSI
short: MSI
weight: 5
---

MSI is the file format and command line utility for the [Windows Installer][installer]. Windows Installer (previously known as Microsoft Installer) is an interface for Microsoft Windows that's used to install and manage software on Windows systems. This page covers installing and managing Vector through the MSI package repository.

## Installation

Install Vector:

```powershell
powershell Invoke-WebRequest https://packages.timber.io/vector/{{< version >}}/vector-x86_64.msi \
  -OutFile vector-{{< version >}}-x86_64.msi && \
  msiexec /i vector-{{< version >}}-x86_64.msi /quiet
```

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Start

```powershell
C:\Program Files\Vector\bin\vector \
  --config C:\Program Files\Vector\config\vector.toml
```

[installer]: https://en.wikipedia.org/wiki/Windows_Installer
