---
description: Build Vector directly from source
---

# Build From Source

Vector is built in [Rust](https://www.rust-lang.org/) which makes compilation simple.

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

5. The `vector` binary is placed in the `target/release` sub-directory.

