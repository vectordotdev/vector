# Package repository publishing

The `Package Repositories` workflow consumes existing GitHub release assets and
publishes apt and yum repositories. `cargo vdev release repositories` provides the
maintainer commands. The container implementation is under
`distribution/repositories`; its image is built within each workflow run.

This workflow is disabled unless the repository variable
`PACKAGE_REPOSITORIES_ENABLED` is `true`. It requires a completed historical
backfill. Enabling it is part of the repository migration cutover, after the new
public key, installation instructions and client migration have been published.
The normal Test Suite runs the offline publisher regressions when this tooling
changes. Those checks require no signing secrets or AWS credentials and remain
available while publication is disabled.

## Inputs and job boundaries

Automatic publication follows a successful `Release Suite` run for a final
`v0.x.y` tag. Manual dispatch requires that run's ID, the same immutable tag, and
an explicit stable or beta channel. Stable rejects prereleases. The input job
checks the repository ID, workflow path, tag push, commit, release state, asset
set, asset digests and SHA256SUMS. It records the six packages in an immutable
manifest. Debian `armel` remains a raw release download, outside these indexes.

The workflow checks out its default-branch code, not the release tag. It builds
one tools image and vdev binary and transfers those exact tools between jobs.
Signing has no AWS credentials. Publishing has no private signing key. Client
verification has neither. Package installation happens only in client jobs.

The following protected environments must exist before activation. Restrict both
to `master`, require the team's release reviewers, and prevent self-approval.

| Environment | Configuration |
| --- | --- |
| `package-repositories-signing` | `PACKAGE_SIGNING_SUBKEY` and `PACKAGE_SIGNING_PASSPHRASE` secrets |
| `package-repositories-publishing` | `PACKAGE_REPOSITORIES_ROLE_ARN` variable, assumed with GitHub OIDC |

Import only the encrypted signing-subkey export from the production custody
record. The primary secret key and revocation certificate do not belong in CI.
The code rejects an available primary secret key, an unexpected subkey, expired
keys, and an unprotected signing subkey. Temporary keyrings and the passphrase
file exist only in container tmpfs and are excluded from artifacts.

The public identity is pinned in `distribution/repositories/production.json`:

- Primary: `FBC83025C7F26E39DBCCDCD9ED518768DAA61968`.
- Signing subkey: `E49714E41E4A57313AACFC333C9F4A9B1ACBAD72`.

## Storage and publication

The production configuration targets `dd-cose-releases` in account
`486234852809`, region `us-east-1`. Live content uses
`vector/repositories/{apt,yum}/`. Internal state, signed retry bundles and
metadata archives use `package-repositories/`, outside the publicly readable
`vector/` prefix. Hosting must map the apt/yum domain roots to the corresponding
live prefixes.

The role needs only scoped S3 listing, reads, conditional writes and access to its
own IAM role metadata. Keep bucket versioning, TLS enforcement and encryption
enabled. Deny object deletion and changes outside these prefixes. The OIDC trust
must bind the repository's immutable ID and the publishing environment. The
role's maximum session duration must be at most one hour; workflow jobs request
15-minute sessions. Credentials are fixed for a publisher invocation and never
refreshed while holding a lock.

Each generation records all live paths, sizes and SHA-256 digests. Preparation
downloads current metadata and new packages; historical package payloads stay
in S3. Apt indexes are merged by native package identity. RPM indexes are merged
with all versions retained. Previous apt SHA-256/SHA-512 by-hash objects and yum
metadata remain available. An outdated preparation or a conflicting immutable
object fails before live metadata changes.

Signed RPMs are saved by the source artifact digest. Signed publication bundles
are saved by input and base-generation digest before live publication. Retries
restore those bytes and skip signing; rebasing against newer metadata can reuse
the saved RPM signatures. Debian package bytes are unchanged.

Publication archives signed metadata, writes immutable packages and indexes,
then replaces canonical indexes and roots. All writers use one conditional S3
control object, including stable, beta, backfill and recovery. Control objects
must have no lifecycle expiration. Retain internal bundles and immutable content
for as long as releases can be retried or rolled back.

The RPM root and detached signature are separate objects. Clients can observe a
temporary mismatch and must fail signature verification during that interval.
Origin writes are not an atomic CDN update. Mutable objects use revalidation
headers; the actual CDN caching and retry behavior must be validated before
cutover. Workflow success also requires public HTTPS checksum verification and
native apt/DNF installation on amd64 and arm64.

## Maintainer commands

Build the tools once from reviewed source:

```sh
cargo vdev release repositories build-image
cargo vdev release repositories test
```

The default workspace is `target/package-repositories`. Use a different `--work`
directory for each release. A configuration override is intended for isolated
staging; it must include the corresponding pinned public key.

```sh
cargo vdev release repositories --work target/repository-v0.60.0 fetch \
  --run 39289726838 --tag v0.60.0 --channel stable
cargo vdev release repositories --work target/repository-v0.60.0 base
cargo vdev release repositories --work target/repository-v0.60.0 prepare
cargo vdev release repositories --work target/repository-v0.60.0 publish
cargo vdev release repositories --work target/repository-v0.60.0 verify
```

`fetch` accepts `GH_TOKEN` for GitHub API access. AWS commands require fixed
`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, and
`PACKAGE_REPOSITORIES_ROLE_ARN`. `prepare` requires the two signing secrets only
when `base` did not restore a signed result. Never put secret values in command
arguments, a tracked configuration file or a release artifact.

For normal retries, dispatch the workflow with the original run, tag and channel.
Do not rebuild packages or move the tag. An older already-published release must
not replace the current repository generation; use an explicit rollback when
that is the intended operation.

## Recovery

Use the manually dispatched `Package Repository Maintenance` workflow on `master`
for `status`, `rollback` or `recover-lock`. It uses the protected publishing
environment, its OIDC role and the same concurrency group as release publication.
It receives no signing secrets. Maintenance remains available when
`PACKAGE_REPOSITORIES_ENABLED` is disabled so an incident can be recovered while
new publication is stopped. The publishing environment and role must be
configured before maintenance can run.

The workflow requires the exact archived snapshot digest for rollback. Lock
recovery additionally requires the exact control-object version and confirmation
that the old owner has stopped. The publisher enforces the same 65-minute safety
window for the workflow and CLI. A successful run records the resulting control
state in its summary; public endpoint and client verification still follow a
rollback.

Inspect the state before acting:

```sh
cargo vdev release repositories status
```

An unexpected error retains the lock because a timed-out S3 write may have
completed. Locks are never stolen automatically. Stop the previous workflow or
process and ensure its in-flight writes have quiesced. Credential expiry alone
does not cancel requests already accepted by S3. After the 65-minute safety
window, recover using the exact inspected S3 control-object version:

```sh
cargo vdev release repositories recover-lock \
  --expected-version VERSION_ID --confirm-owner-stopped
```

Resume the pending signed snapshot, or restore the last completed snapshot:

```sh
cargo vdev release repositories rollback SNAPSHOT_SHA256
```

Rollback restores retained signed metadata and checks that its immutable objects
still exist. It does not downgrade installed packages or restore Datadog-only
client trust. Verify the public endpoints and native clients after rollback.
Never delete a control object to bypass the recovery checks.
