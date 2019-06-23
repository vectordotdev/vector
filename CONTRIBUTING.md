# Contributing

First, thank you for contributing to Vector! We know how important a
strong community is to open source and we welcome anyone that is willing
to particpate.

## Prerequisites

1. **You are familiar with the [docs](https://docs.vectorproject.io).**
2. **You have read the [code of conduct](/CODE_OF_CONDUCT.md)**

## Workflow

1. [Github issues][issues] for bug and feature tracking.
2. Github [milestones] are used for Roadmap planning.
3. All new work should be contained in a branch.
4. Pull requests are submittted for review. See the
   [pull request process](#pull-request-process).

## Getting Started

Vector maintains a [`Meta: Good first issue` label][good_first_issues].
These are excellent simple issues that will help you get acclimated with
the Vector project.

## Pull Request Process

- [ ] Update the [`CHANGELOG.md`](/CHANGELOG.md) if necessary.
- [ ] Update the [`scripts/config_schema.toml`](/scripts/config_schema.toml)
      if necessary. Run `cargo make docs` sync changes across the docs.
- [ ] You may merge the Pull Request once you have an approved pull request
      review from a team member.

## Developing

Please see the [develoment](/DEVELOPMENT.md) guide.

[good_first_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Meta%3A+Good+first+issue%22
[issues]: https://github.com/timberio/vector/issues
[milestones]: https://github.com/timberio/vector/milestones?direction=asc&sort=title&state=open