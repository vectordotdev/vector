---
description: Build Vector directly from source
---

# Build From Source

Vector compiles to a single static binary, making it simple to install.
There is no runtime and there are no dependencies.

## Installation

1. [Install Rust](https://www.rust-lang.org/tools/install):  

    ```bash
    curl https://sh.rustup.rs -sSf | sh
    ```

2. Clone to [Vector repo](https://github.com/timberio/vector):  

    ```bash
    git clone git@github.com:timberio/vector.git
    ```

3. Change into the Vector directory:  

    ```bash
    cd vector
    ```

4. Build Vector:  

    ```bash
    cargo build --release
    ```

5. Start Vector:

    ```bash
    target/release/vector --config=/path/to/vector.toml
    ```

    The `vector` binary is placed in the `target/release` sub-directory.

6. See (How It Works)(#how-it-works) for optional follow up tasks.


## How It Works

### Data Directory

We highly recommend creating a data director that Vector can use:

```
mkdir /var/lib/vector
```

And in your `vector.toml` file:

```toml
data_dir = "/var/lib/vector"
```

### Service Managers

#### Init.d

Vector includes a [`vector` init.d file][vector_initd] that you
can use to manage Vector through init.d.

#### Systemd

Vector includes a [`vector.service` Systemd file][vector_systemd] that you
can use to manage Vector through Systemd.


[vector_initd]: https://github.com/timberio/vector/blob/master/distribution/init.d/vector
[vector_systemd]: https://github.com/timberio/vector/blob/master/distribution/systemd/vector.service

