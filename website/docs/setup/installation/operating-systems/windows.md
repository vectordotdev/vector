---
title: Install Vector On MacOS
sidebar_label: MacOS
description: Install Vector on MacOS
---

Vector can be installed on Windows from an archive. The installation procedure
is described below:

Download Vector release archive:

```powershell
Invoke-WebRequest http://127.0.0.1:8000/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
```

Extract files from the archive:

```powershell
Expand-Archive vector-x86_64-pc-windows-msvc.zip .
```

Navigate to Vector directory:

```powershell
cd vector-x86_64-pc-windows-msvc
```

Start Vector:

```powerhsell
bin\vector.exe --config config\vector.toml
```



