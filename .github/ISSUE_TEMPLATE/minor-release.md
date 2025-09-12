---
name: Vector minor release
about: Use this template for a new minor release.
title: "Vector [version] release"
labels: "domain: releasing"
---


# Setup and Automation

Note the preparation steps are now automated. First, alter/create release.env

```shell
export NEW_VECTOR_VERSION=<new Vector version> # replace this with the actual new version (e.g.: 0.50.0)
export NEW_VRL_VERSION=<new VRL version> # replace this with the actual new VRL version (e.g.: 0.30.0)
export MINOR_VERSION=$(echo "$NEW_VECTOR_VERSION" | cut -d. -f2)
export PREP_BRANCH=prepare-v-0-"${MINOR_VERSION}"-"${NEW_VECTOR_VERSION}"-website
export RELEASE_BRANCH=v0."${MINOR_VERSION}"
```

and then source it by running `source ./release.env`

# The week before the release

## 1. Manual Steps

- [ ] Cut a new release of [VRL](https://github.com/vectordotdev/vrl) if needed
  - VRL release steps: https://github.com/vectordotdev/vrl/blob/main/release/README.md

## 2. Automated Steps

Run the following:

```shell
cargo vdev release prepare --version "${NEW_VECTOR_VERSION}" --vrl-version "${NEW_VRL_VERSION}"
```

Automated steps include:
- [ ] Create a new release branch from master to freeze commits
  - `git fetch && git checkout origin/master && git checkout -b "{RELEASE_BRANCH}" && git push -u`
- [ ] Create a new release preparation branch from `master`
  - `git checkout -b "${PREP_BRANCH}" && git push -u`
- [ ] Pin VRL to latest released version rather than `main`
- [ ] Check if there is a newer version of [Alpine](https://alpinelinux.org/releases/) or
      [Debian](https://www.debian.org/releases/) available to update the release images in
      `distribution/docker/`. Update if so.
- [ ] Run `cargo vdev build release-cue` to generate a new cue file for the release
  - [ ] Copy VRL changelogs from the VRL version in the last Vector release as a new changelog entry
        ([example](https://github.com/vectordotdev/vector/blob/9c67bba358195f5018febca2f228dfcb2be794b5/website/cue/reference/releases/0.41.0.cue#L33-L64))
- [ ] Update version number in `website/cue/reference/administration/interfaces/kubectl.cue`
- [ ] Update version number in `distribution/install.sh`
- [ ] Add new version to `website/cue/reference/versions.cue`
- [ ] Create new release md file by copying an existing one in `./website/content/en/releases/` and
      updating version number
- [ ] Commit these changes
- [ ] Open PR against the release branch (`"${RELEASE_BRANCH}"`) for review

## 3. Manual Steps

- [ ] Edit `website/cue/reference/releases/"${NEW_VECTOR_VERSION}".cue`
  - [ ] Add description key to the generated cue file with a description of the release (see
        previous releases for examples).
  - [ ] Ensure any breaking changes are highlighted in the release upgrade guide
  - [ ] Ensure any deprecations are highlighted in the release upgrade guide
  - [ ] Review generated changelog entries to ensure they are understandable to end-users
- [ ] Check for any outstanding deprecation actions in [DEPRECATIONS.md](https://github.com/vectordotdev/vector/blob/master/docs/DEPRECATIONS.md) and
    take them (or have someone help you take them)
- [ ] PR review & approval

# On the day of release

- [ ] Make sure the release branch is in sync with origin/master and has only one squashed commit with all commits from the prepare branch. If you made a PR from the prepare branch into the release branch this should already be the case
  - [ ] `git checkout "${RELEASE_BRANCH}"`
  - [ ] `git show --stat HEAD` - This should show the squashed prepare commit
  - [ ] `git diff HEAD~1 origin/master --quiet && echo "Same" || echo "Different"` - Should output `Same`
  - Follow these steps if the release branch needs to be updated
    - [ ] Rebase the release preparation branch on the release branch
      - [ ] Squash the release preparation commits (but not the cherry-picked commits!) to a single
          commit. This makes it easier to cherry-pick to master after the release.
      - [ ] Ensure release date in `website/cue/reference/releases/0.XX.Y.cue` matches current date.
        - If this needs to be updated commit and squash it in the release branch
    - [ ] Merge release preparation branch into the release branch
        - `git switch "${RELEASE_BRANCH}" && git merge --ff-only "${PREP_BRANCH}"`

- [ ] Tag new release
  - [ ] `git tag v"${NEW_VECTOR_VERSION}" -a -m v"${NEW_VECTOR_VERSION}"`
  - [ ] `git push origin v"${NEW_VECTOR_VERSION}"`
- [ ] Wait for release workflow to complete
  - Discoverable via [release.yml](https://github.com/vectordotdev/vector/actions/workflows/release.yml)
- [ ] Reset the `website` branch to the `HEAD` of the release branch to update https://vector.dev
  - [ ] `git switch website && git reset --hard origin/"${RELEASE_BRANCH}" && git push`
  - [ ] Confirm that the release changelog was published to https://vector.dev/releases/
    - The deployment is done by Amplify. You can see
      the [deployment logs here](https://dd-corpsite.datadoghq.com/logs?query=service%3Awebsites-vector%20branch%3Awebsite&agg_m=count&agg_m_source=base&agg_t=count&cols=host%2Cservice&fromUser=true&messageDisplay=inline&refresh_mode=sliding&storage=hot&stream_sort=time%2Casc&viz=stream).
- [ ] Release Linux packages. See [`vector-release` usage](https://github.com/DataDog/vector-release#usage).
  - Note: the pipeline inputs are the version number `v"${NEW_VECTOR_VERSION}"` and a personal GitHub token.
  - [ ] Manually trigger the `trigger-package-release-pipeline-prod-stable` job.
- [ ] Release updated Helm chart. See [releasing Helm chart](https://github.com/vectordotdev/helm-charts#releasing).
- [ ] Once Helm chart is released, updated Vector manifests
    - Run `cargo vdev build manifests` and open a PR with changes
- [ ] Add docker images to [https://github.com/DataDog/images](https://github.com/DataDog/images/tree/master/vector) to have them available internally. ([Example PR](https://github.com/DataDog/images/pull/7104))
- [ ] Create a new PR with title starting as `chore(releasing):`
  - [ ] Cherry-pick any release commits from the release branch that are not on `master`, to `master`
  - [ ] Bump the release number in the `Cargo.toml` on master to the next minor release.
  - [ ] Also, update `Cargo.lock` with: `cargo update -p vector`
  - [ ] If there is a VRL version update, revert it and make it track the git `main` branch and then run `cargo update -p vrl`.
- [ ] Kick-off post-mortems for any regressions resolved by the release
