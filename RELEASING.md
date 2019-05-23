# Releasing

This document will cover how to properly release Vector.

Vector adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html) and the release
process is dependent on the version change. In addition, it's important to understand the
branch structure:

* `master` - Reflects the latest `MAJOR.MINOR-dev` version.
* `vMAJOR.MINOR` - Contains `MAJOR.MINOR` specific changes and point releases.

## Quick Start

### Patch Releases

1. Create a new branch from the latest `vMAJOR.MINOR.PATCH` tag. Ex: `git checkout -b v1.2.3 v1.2.2`
2. Make the appropriate changes/fixes.
3. Update the `version` key in [`/Cargo.toml`]
4. Update the [`/CHANGELOG.md`] header to reflect the new version `vMAJOR.MINOR.PATCH - 2019-05-02`
5. Commit the changes above with message "Release vMAJOR.MINOR.PATCH"
6. Create a new tag named `vMAJOR.MINOR.PATCH`
7. Push the new tag
8. Delete the temporary branch you created.

### Major/Minor Releases

1. Switch to the `master` branch, this should be reflective of the new version's changes.
2. Update the `version` key in [`/Cargo.toml`]
3. Update the [`/CHANGELOG.md`] header to reflect the new version `vMAJOR.MINOR.0 - 2019-05-02`
4. Commit the changes above with message `"Release vMAJOR.MINOR.PATCH"`
5. Create a new tag named `vMAJOR.MINOR.PATCH`
6. Push the new tag.
7. Update the [`/CHANGELOG.md`] header to reflect the new upcoming version `vNEW_MAJOR.NEW_MINOR-dev`
8. Commit the changes above with message `"Start vNEW_MAJOR.NEW_MINOR+1"`
9. ![](https://i.giphy.com/media/3ohzdIvnUKKjiAZTSU/giphy.webp)


[`/Cargo.toml`]: /Cargo.toml
[`/CHANGELOG.md`]: /CHANGELOG.md
