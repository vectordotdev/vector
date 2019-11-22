---
title: Install Vector On Windows
sidebar_label: Windows
description: Install Vector on Windows
---

Vector can be installed on Windows from an archive or source.

## Install from archive using PowerShell

1.  Download Vector release archive (latest):

    ```powershell
    Invoke-WebRequest https://packages.timber.io/vector/latest/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
    ```

    Download Vector release archive (nightly):

    ```powershell
    Invoke-WebRequest https://packages.timber.io/vector/nightly/latest/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
    ```
2.  Extract files from the archive:

    ```powershell
    Expand-Archive vector-x86_64-pc-windows-msvc.zip .
    ```

3.  Navigate to Vector directory:

    ```powershell
    cd vector-x86_64-pc-windows-msvc
    ```
4.  Start Vector:

    ```powerhsell
    bin\vector.exe --config config\vector.toml
    ```

## Install from archive manually

1. [Download](https://vector.dev/download) release archive manually.
2. Extract its content using the system context menu.
3. Open command prompt and run
    ```
    cd "<full path of Vector>"
    ```
4. Start Vector:
    ```
    bin\vector.exe --config config\config.toml
    ```



