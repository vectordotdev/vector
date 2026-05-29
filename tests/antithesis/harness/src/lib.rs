//! Shared harness code lives here once there is more than one scenario to share
//! it. Today there is a single scenario, `vector_to_vector_e2e_disk`, and each
//! test-command binary in `src/bin/` is self-contained, so there is nothing to
//! factor out yet. When a second scenario arrives, lift the common HTTP and
//! oracle helpers into modules here.
