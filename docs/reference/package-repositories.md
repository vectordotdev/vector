# Package repository publishing

The `Package Repositories` workflow consumes existing GitHub release assets and
publishes apt and yum repositories. `cargo vdev release repositories` provides the
maintainer commands. The container implementation is under
`distribution/repositories`; its image is built within each workflow run.

This workflow is disabled unless the repository variable
`PACKAGE_REPOSITORIES_ENABLED` is `true`. It requires a completed historical
backfill. Enabling it is part of the repository migration cutover, after the new
public key, installation instructions and client migration have been published.
The normal Test Suite checks the vdev CLI and builds the publisher image when
this tooling changes. Those checks require no signing secrets or AWS credentials.
Publisher regressions and migration rehearsals live in the private CI sandbox.

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

## Client setup and migration

`distribution/repositories/setup.sh` is the standalone apt/yum setup script.
It configures a repository without installing or upgrading Vector. A new source
defaults to stable and COSE-only trust. An existing `vector.list` or `vector.repo`
keeps its URL, channel, architecture restrictions and other options while adding
COSE trust. `--migrate` requires an existing managed source.

A pre-cutover migration retains the old public keys in Vector's own key file.
It leaves shared Datadog keyrings and RPM database entries unchanged. On RPM
clients, old remote public keys are copied into the local Vector key file so
subsequent refreshes no longer need Datadog's key endpoint. Both `gpgcheck` and
`repo_gpgcheck` are required, including on custom mirrors. The old RHEL/CentOS
8.1 exception is not retained.

Only after hosting has switched to COSE, `--migrate --cose-only` can remove legacy
trust from Vector's own configuration. This mode also supports late migration
when the old key files or endpoints are unavailable. It never deletes shared
Datadog key files, RPM keys or keys cached by DNF.

The default public key URL is
`https://dd-cose-releases.s3.us-east-1.amazonaws.com/vector/keys/vector-packages.asc`.
The script verifies its full primary fingerprint before changing managed files.
It uses HTTPS with certificate verification for key downloads, backs up original
managed files with `.pre-cose`, installs replacements atomically, and refreshes
only the Vector repository. A failed final refresh leaves the saved configuration
available for inspection and retry. The script rejects ambiguous source layouts,
symlinked managed files, unavailable legacy trust and unsupported overrides
before replacing source files. Custom filenames and deb822 sources require
manual migration.

Fresh setup retains `VECTOR_DIST_CHANNEL`, `VECTOR_MAJOR_VERSION`,
`VEC_REPO_URL`, `TESTING_APT_URL`, `TESTING_YUM_URL`,
`TESTING_APT_REPO_VERSION` and `TESTING_YUM_VERSION_PATH`.
`TESTING_KEYS_URL` is a compatibility alias for the HTTPS directory containing
`vector-packages.asc`. An owned staging mirror can supply
`VECTOR_PUBLIC_KEY_URL` and an explicit full
`VECTOR_PUBLIC_KEY_FINGERPRINT`. These are public trust settings, not secrets.
Existing sources reject fresh-setup overrides rather than switching their
channel on a repeated invocation. `CSM_MIGRATE=true` disables the known
`timber-vector` file with a backup.

The publisher keeps the existing apt identity: no `Origin` or `Label`, an empty
`Suite`, and the channel as `Codename`. This permits the signing-key and hosting
transition without a separate release-information approval. The observed
legacy fields and the regression check comparing them with publisher output
are kept in the private migration test suite.

Before announcement, deploy the reviewed script at
`vector/setup.sh` and the public key at `vector/keys/vector-packages.asc`
in the COSE bucket, then publish the migration guide. These paths are separate
from the package publisher's scope and need an appropriately scoped deployment.
Keep the live `setup.vector.dev` redirect and normal installation pages unchanged
until the repository hosting cutover. At cutover, point `setup.vector.dev` at the
new script and update those installation pages together.

The protected production client job runs this setup script before installing a
release. Activation therefore requires the public key endpoint to be available.
Private sandbox tests use separate ephemeral legacy and COSE signing identities
for disposable HTTPS clients. Private keys stay in tmpfs; client bundles contain
only public keys and signed packages.

## Maintainer commands

Build the tools once from reviewed source:

```sh
cargo vdev release repositories build-image
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
for `status`, `rollback` or `recover-lock`. It shares the release workflow's
concurrency group. Status and lock recovery use only the protected publishing
environment and its OIDC role. Rollback downloads archives there, signs in the
protected signing environment, then publishes and verifies public HTTPS in
separate jobs. Signing never receives AWS credentials; publishing never receives
the private key.

Maintenance remains available when `PACKAGE_REPOSITORIES_ENABLED` is disabled.
The protected environments and role must be configured before it can run.
Rollback requires an exact archived snapshot digest. Lock recovery also requires
the exact control-object version and confirmation that the old owner stopped.
A successful operation records the resulting control state in its summary.

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

Resume the pending signed publication, or select an archived package set. When
abandoning an incomplete publication, the selection must be the last completed
snapshot. The maintenance workflow performs these phases with separate
credentials; the equivalent CLI sequence uses a fresh workspace:

```sh
cargo vdev release repositories --work target/repository-rollback rollback base SNAPSHOT_SHA256
cargo vdev release repositories --work target/repository-rollback rollback prepare
cargo vdev release repositories --work target/repository-rollback rollback publish
cargo vdev release repositories --work target/repository-rollback rollback verify
```

Skip `prepare` when `base` restores a saved signed result. Rollback signs a new apt
Release date, newer than the selected, active and any pending roots. It preserves
the selected indexes and repository identity, reuses the selected yum signatures,
and retains all completed immutable history. Publication rejects a changed base
or pending state. Interrupted and completed retries restore the same signed
bundle without signing again or creating extra live-object versions.

Replaying old apt signatures alone is insufficient: clients can keep their newer
cached indexes when the restored Release date is older, as shown in
[apt's metadata verification](https://sources.debian.org/src/apt/2.2.4/apt-pkg/acquire-item.cc/).
Neither cache deletion nor disabling date/signature checks is part of recovery.
A clock more than five seconds behind a required new date stops preparation;
correct the clock or retry later. Repositories with `Valid-Until` or a changed
metadata layout require a separate recovery policy.

Rollback changes which versions the indexes advertise. It does not downgrade
installed packages or restore Datadog-only trust. To restore a newer package
selection afterward, dispatch rollback with that newer archived snapshot; do not
replay its old publication. Run native client checks after public verification.
Never delete a control object to bypass recovery checks.
