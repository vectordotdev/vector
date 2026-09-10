---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---

# Before preparation

- [ ] Cut a new [VRL release](https://github.com/vectordotdev/vrl/blob/main/release/README.md) if needed.
- [ ] Choose the Vector release version and the released VRL version.
- [ ] Activate the [Release freeze](https://github.com/vectordotdev/vector/rules/22807978)
      ruleset. Keep it active until automated housekeeping has merged.

# Prepare the release

- [ ] Run the [Prepare release](https://github.com/vectordotdev/vector/actions/workflows/release_prepare.yml) workflow with the Vector and released VRL versions.
- [ ] Review the generated release notes in the bot-authored PR, including changelog entries, breaking changes, deprecations, and upgrade guidance.
- [ ] Use **Bypass rules and merge** to squash-merge the preparation PR directly into `master`.
- [ ] After the automated housekeeping PR merges, disable the release freeze.

The merge is the release approval. The autotag workflow validates the merged diff and creates the
version tag at the exact squash-merge commit. That tag starts the existing release workflow.
After publication, housekeeping restores VRL main, refreshes dependency files, and merges only its
exact generated commit after required checks pass.

# Downstream releases

These remain independently operated downstream channels; their failure does not change the Vector
tag or rebuild its artifacts.

- [ ] Publish the website from the release tag and confirm the release page is live.
- [ ] Release Linux packages. Refer to the internal releasing document.
- [ ] Release the updated Helm chart. See the
      [Helm chart release instructions](https://github.com/vectordotdev/helm-charts/blob/develop/RELEASING.md).
- [ ] After the Helm chart release completes, run `cargo vdev build manifests` and open a Vector PR
      with the generated manifest updates.
- [ ] Release Homebrew. Refer to the internal releasing document.
- [ ] Update the GitHub release description and send the release announcement.
