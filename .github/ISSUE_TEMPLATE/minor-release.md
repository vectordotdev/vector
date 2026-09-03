---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

# Before preparation

- [ ] Cut a new [VRL release](https://github.com/vectordotdev/vrl/blob/main/release/README.md) if needed.
- [ ] Choose the Vector release version and the released VRL version.
- [ ] Decide whether the Alpine or Debian release image needs an explicit base-image update.

# Prepare the release

- [ ] Run the [Prepare release](https://github.com/vectordotdev/vector/actions/workflows/release_prepare.yml) workflow with:
  - `version`: the stable Vector version, for example `0.59.0`.
  - `vrl_version`: the exact released VRL version.
  - Optional Alpine and Debian versions when those base images need updating.
- [ ] Review the bot-authored `release/prepare-v*` PR.
  - [ ] Confirm its release-state validation check passes.
  - [ ] Review the generated changelog entries and remove reverted or housekeeping entries.
  - [ ] Review breaking changes, deprecations, and upgrade guidance.
  - [ ] Confirm the release date and pinned VRL version.
  - [ ] Run `cargo vdev deprecation show --version "<version>"`.
- [ ] Squash-merge the preparation PR into `master`.

The merge is the release approval. The autotag workflow validates the merged diff and creates the
version tag at the exact squash-merge commit. That tag starts the existing release workflow.

# Downstream releases

These remain independently operated downstream channels; their failure does not change the Vector
tag or rebuild its artifacts.

- [ ] Publish the website from the release tag and confirm the release page is live.
- [ ] Release Linux packages. Refer to the internal releasing document.
- [ ] Release Homebrew. Refer to the internal releasing document.
- [ ] Update the GitHub release description and send the release announcement.
