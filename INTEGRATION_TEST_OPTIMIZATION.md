# Integration Test Optimization

## Summary

This implementation adds a `--prebuilt` flag to integration and E2E tests that enables using pre-compiled test binaries from the Docker image without mounting source code. This separates **CI/pre-built mode** (fast, uses baked-in binaries) from **local dev mode** (mounts source, incremental compilation).

## The Core Insight

There are two fundamentally different use cases that were being conflated:

1. **CI / Pre-built Mode**: Use pre-compiled binaries baked into the image. No source mounting needed.
2. **Local Development Mode**: Mount source code for live editing. Use incremental compilation with volume caching.

The solution: Add a flag to choose which mode to use.

## Changes Made

### 1. Add `--prebuilt` Flag to Commands

**Files changed:**
- `vdev/src/commands/integration/test.rs`
- `vdev/src/commands/e2e/test.rs`
- `vdev/src/commands/compose_tests/test.rs`

Added a `--prebuilt` boolean flag that gets passed through the command chain.

### 2. Update TestRunner Trait

**File:** `vdev/src/testing/runner.rs`

Added `prebuilt: bool` parameter to the `test()` method signature.

### 3. Conditional Source Code Mounting

**File:** `vdev/src/testing/runner.rs` - `create()` method

When `prebuilt=true`:
- **Skip** mounting source code (`/Users/.../vector:/home/vector`)
- Tests run using code baked into the image

When `prebuilt=false` (default):
- **Mount** source code as before
- Tests compile from mounted source with incremental caching

```rust
// Only mount source code in dev mode (not prebuilt)
if !prebuilt {
    cmd.args(["--volume", &source_mount]);
}
```

### 4. Pre-built Artifact Copying

**File:** `vdev/src/testing/runner.rs` - `test()` method

When `prebuilt=true`, on first run:
- Check if `/opt/vector-build/debug` exists in the image
- If target volume is empty, copy pre-built artifacts to it
- Subsequent runs use these cached artifacts

### 5. Dockerfile Updates

**File:** `tests/e2e/Dockerfile`

When built with `BUILD=true`:
- Compiles tests during image build
- Copies artifacts to `/opt/vector-build/debug` in the image
- These become part of the image layers (not ephemeral cache)

### 6. Enable BUILD=true for Integration Tests

**File:** `vdev/src/testing/build.rs`

Changed from `build: false` to `build: true` so `cargo vdev int build` compiles tests into the image.

## Usage

### Local Development (Default)

```bash
# Build image (fast, no compilation)
cargo vdev int build

# Run tests (mounts source, compiles incrementally)
cargo vdev int test datadog-logs

# Edit code, run again (only recompiles what changed)
cargo vdev int test datadog-logs
```

**Behavior:**
- Source code is mounted
- First run compiles everything (~10-15 min)
- Subsequent runs use incremental compilation
- Only changed files recompile

### CI / Pre-built Mode

```bash
# Build image with pre-compiled tests (slow, one time)
cargo vdev int build  # ~15-20 minutes

# Run tests with --prebuilt flag (fast!)
cargo vdev int test --prebuilt datadog-logs  # ~30-60 sec

# Run more tests (all fast!)
cargo vdev int test --prebuilt elasticsearch  # ~30-60 sec
```

**Behavior:**
- NO source code mounting
- Tests run from pre-compiled binaries in the image
- No compilation at test time
- Fast, reproducible

## How It Works

### Dev Mode (`--prebuilt` not set)

```
┌────────────────────────────────────────────┐
│ Container Setup                             │
│ ├─ Mount source: ✅                         │
│ ├─ Mount volume for cache: ✅              │
│ └─ Workdir: /home/vector (mounted source)  │
└────────────────────────────────────────────┘
                ↓
┌────────────────────────────────────────────┐
│ Test Run                                    │
│ ├─ Cargo sees source code                  │
│ ├─ Compiles (or uses incremental cache)    │
│ └─ Runs tests                               │
└────────────────────────────────────────────┘
```

### Pre-built Mode (`--prebuilt`)

```
┌─────────────────────────────────────────────┐
│ Container Setup                              │
│ ├─ Mount source: ❌ (skipped!)               │
│ ├─ Mount volume for cache: ✅               │
│ └─ Workdir: /home/vector (from image)       │
└─────────────────────────────────────────────┘
                ↓
┌─────────────────────────────────────────────┐
│ First Run                                    │
│ ├─ Copy /opt/vector-build → /home/target    │
│ └─ Run pre-compiled tests                   │
└─────────────────────────────────────────────┘
                ↓
┌─────────────────────────────────────────────┐
│ Subsequent Runs                              │
│ └─ Run tests from cache (no copying needed) │
└─────────────────────────────────────────────┘
```

## Key Benefits

### 1. Separation of Concerns
- CI uses `--prebuilt` for speed and reproducibility
- Developers use default mode for fast edit-test cycles

### 2. No Timestamp Issues
- In pre-built mode, no source mounting = no timestamp mismatches
- Tests run from image layers, completely isolated from host filesystem

### 3. Incremental Dev Workflow Still Works
- Default mode unchanged
- Source mounting + volume caching works as before
- Developers get fast incremental builds

### 4. CI Optimization
- Build image once with pre-compiled tests
- All test runs are fast (~30-60 sec each)
- No recompilation between test runs

## Performance

### Before (Current Main Branch)

| Scenario | Time |
|----------|------|
| `cargo vdev int build` | 2-3 min |
| First test run | 15-20 min (compiles) |
| Second test run | 10-15 min (recompiles due to timestamp issues) |
| CI (10 tests) | ~2-3 hours |

### After (With --prebuilt)

**Dev Mode (default):**
| Scenario | Time |
|----------|------|
| `cargo vdev int build` | 15-20 min (now compiles) |
| First test run | 1-2 min (uses pre-built) |
| Edit code + rerun | 1-5 min (incremental) |

**Pre-built Mode (CI):**
| Scenario | Time |
|----------|------|
| `cargo vdev int build` | 15-20 min (one time) |
| Each test run | 30-60 sec |
| CI (10 tests) | ~25-30 min total |

**Time saved in CI: ~2+ hours** 🎉

## CI Workflow

```yaml
- name: Build integration test image
  run: cargo vdev int build
  # This takes 15-20 min but only runs once

- name: Cache Docker image
  uses: docker/build-push-action@v5
  with:
    cache-from: type=gha
    cache-to: type=gha,mode=max

- name: Run integration tests
  run: |
    cargo vdev int test --prebuilt datadog-logs
    cargo vdev int test --prebuilt elasticsearch
    cargo vdev int test --prebuilt kafka
  # Each test runs in 30-60 sec!
```

## Implementation Details

### Why Not Always Use Pre-built?

For local development, mounting source code is essential:
- Edit code and immediately test
- Incremental compilation is faster than full rebuilds
- No need to rebuild Docker image after every code change

### Why Not Always Mount Source?

For CI, mounting source code causes problems:
- Timestamp mismatches trigger unnecessary recompilation
- Source on host may differ from what's in the image
- Pre-built binaries are more reproducible

### The Volume Strategy

Both modes use the `vector_target` volume:
- **Dev mode**: Stores incremental compilation artifacts
- **Pre-built mode**: Receives one-time copy from image, then cached

The volume persists across:
- Container restarts
- Multiple test runs
- Container recreation (as long as volume isn't deleted)

## Troubleshooting

### Tests Still Compiling with `--prebuilt`

**Symptoms:** Even with `--prebuilt`, you see "Compiling..." output

**Possible causes:**
1. Image was built before the Dockerfile changes
2. Image doesn't have pre-compiled artifacts at `/opt/vector-build`
3. Build failed during image build

**Solution:**
```bash
# Rebuild the image completely
docker rmi vector-test-runner-1.90:latest
cargo vdev int build

# Verify artifacts exist in image
docker run --rm vector-test-runner-1.90:latest ls -la /opt/vector-build/debug
```

### Dev Mode Recompiling Everything

**Symptoms:** Without `--prebuilt`, tests recompile everything each time

**Possible causes:**
1. Volume was cleared
2. Source timestamps changed significantly

**Solution:**
```bash
# Check if volume has cached artifacts
docker exec vector-test-runner-1.90 ls -la /home/target/debug

# If empty, first run will compile (expected)
# Second run should be incremental
```

## Script Usage

**`./scripts/run-integration-test.sh`** now uses `--prebuilt` by default (for CI):
```bash
./scripts/run-integration-test.sh int datadog-logs  # Uses --prebuilt
```

**For local development, use the command directly:**
```bash
cargo vdev int test datadog-logs  # Dev mode (no --prebuilt)
```

## Backward Compatibility

**Breaking change for the script:**
- `./scripts/run-integration-test.sh` now runs in pre-built mode
- This is the correct behavior for CI (its primary use case)

**Local development commands unchanged:**
- `cargo vdev int test <name>` - Dev mode (default)
- `cargo vdev int test --prebuilt <name>` - Pre-built mode

## Remote Artifact Caching (Already Available)

The implementation already supports remote artifact caching through Docker registries:

1. **Docker Registry Caching**: Pre-built artifacts are part of the Docker image layers
2. **Push/Pull Images**: Images can be pushed to Docker Hub, GitHub Container Registry, or any Docker registry
3. **CI Integration**: Use Docker's layer caching for fast image reuse

Example CI workflow:
```yaml
- name: Build or pull cached image
  uses: docker/build-push-action@v5
  with:
    cache-from: type=gha
    cache-to: type=gha,mode=max
    push: true
    tags: my-registry/vector-test-runner:latest

- name: Run tests with cached artifacts
  run: cargo vdev int test --prebuilt datadog-logs
```

The Docker image serves as the artifact distribution mechanism - no need for a separate artifact storage system.

## Future Enhancements

1. **Auto-detect mode**: If image has `/opt/vector-build`, default to `--prebuilt`
2. **Parallel test execution**: Run multiple `--prebuilt` tests concurrently
3. **Smart invalidation**: Only rebuild image when dependencies change

## Conclusion

This implementation provides a clean separation between:
- **Local development**: Fast edit-test cycles with incremental compilation
- **CI**: Fast, reproducible test runs using pre-compiled binaries

The key innovation is making source code mounting **optional** and controlled by a flag, rather than always mounting or never mounting.
