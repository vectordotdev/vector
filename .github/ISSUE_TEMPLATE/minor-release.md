---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

# Before preparation

- [ ] Cut a new [VRL release](https://github.com/vectordotdev/vrl/blob/main/release/README.md) if needed.
- [ ] Choose the Vector release version and the released VRL version.

# Prepare the release

- [ ] Run the [Prepare release](https://github.com/vectordotdev/vector/actions/workflows/release_prepare.yml) workflow with the Vector and released VRL versions.
- [ ] Review the generated release notes in the bot-authored PR, including changelog entries, breaking changes, deprecations, and upgrade guidance.
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
