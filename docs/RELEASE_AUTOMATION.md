# Release automation

A maintainer starts **Prepare release** with the Vector and released VRL versions, reviews the
resulting PR, and squash-merges it into `master`. The approved merge is tagged and published.
After publication succeeds, housekeeping bumps Vector to the next minor `-dev` version, restores
VRL tracking to `main`, and regenerates dependency documentation and licenses.

Housekeeping is generated from current `master`, not from an existing PR's contents. Retries append
commits without rewriting published history. The merge job waits for required checks and submits
only the exact generated head SHA. GitHub must still enforce required checks and base freshness.

## One-time repository setup

These are administrator changes, not workflow steps. Do not enable broad administrator or status-check
bypasses to make the automation pass.

1. Install `vectordotdev-bot` on the repository with **Contents: write** and **Pull requests: write**.
   Provide the `GH_APP_VECTORDOTDEV_BOT_CLIENT_ID` and
   `GH_APP_VECTORDOTDEV_BOT_APP_PRIVATE_KEY` Actions secrets.
2. Add the app to the release **tag creation** ruleset's bypass list. For Vector this is ruleset
   `1612468`. Keep the separate update/deletion ruleset (`1612469`) in force **without** an app bypass;
   release tags must remain immutable.
3. In `master` branch protection, add the app to the **required pull request review** bypass list.
   Also allow it in the restricted-push app list if pushes/merges are restricted. Keep required checks,
   strict/up-to-date checks, and all other protections enabled. This is an app-wide review privilege
   on `master`, not a branch-name-scoped privilege: protect the app credentials and changes to release
   workflows accordingly. Do not grant a general ruleset bypass on `master`.
4. Require the GitHub Actions check **Validate release state transition**, alongside the existing
   required checks. The workflow emits a skipped check for ordinary PRs and merge groups, rather
   than using a path filter that could leave a required check missing.

The ordinary review gate still applies to preparation PRs: automation never requests their merge.
The housekeeping merger uses the REST merge endpoint with an expected SHA, not `--admin` and not
`--auto`. Enabling auto-merge alone does not satisfy a required code-owner approval.

## Recovery

- **Preparation failed before pushing:** rerun Prepare release. Runner-local partial changes are gone.
- **Preparation PR already open:** rerunning preparation leaves it unchanged. Update it from master
  normally; `prepared_from` records the original base and must remain an ancestor. Review any new
  changes included from master and refresh the release notes when needed.
- **Branch pushed but preparation PR creation failed:** inspect the branch, then open its preparation
  PR manually against master. Preflight intentionally fails closed on an orphaned preparation branch.
- **Publication failed:** use **Re-run failed jobs**, not Re-run all jobs. A published GitHub release
  and some external artifacts may already exist. Do not delete or move the release tag.
- **Housekeeping generation/checks/merge failed:** rerun the **Open post-release housekeeping PR** job
  and its dependents (`gh run rerun --job <job-id>`). This regenerates from current master and restores
  VRL main plus the corresponding docs/licenses. It does not repeat publication. The workflow stops
  if required checks fail, the PR head changes, or master advances before the merge; it never forces
  a merge. Rerunning only the merge job cannot refresh an outdated candidate.
- **Housekeeping already merged or master has advanced to a later release:** the state check skips
  generation and merging.

Patch releases retain the existing release-branch process and do not run minor-release housekeeping.
Website, downstream package repositories, Helm, Homebrew, and announcements remain separate steps.

## Validation scope

Unit/integration tests exercise state validation and file restrictions. `ci-sandbox` tests the real
GitHub preparation PR, review gate, tag push, publication-success dependency, and exact-head
housekeeping merge. Build, dependency-file generation, and publication steps are mocked there;
that proves orchestration and interactions, **not** artifact correctness. Keep the sandbox's copied
workflows, shell helpers, and Rust validator synchronized with the implementation under test.
