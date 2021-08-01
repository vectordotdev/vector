# Security Policy

---

<p align="center">
  <strong>Reporting a vulnerability? See the <a href="#vulnerability-reporting">Vulnerability Reporting section</a></strong>
</p>

---

We understand that many users place a high level of trust in Vector to collect
and ship mission-critical data. The security of Vector is a top priority.
That's why we apply widely accepted best practices when it comes to security.
This document will describe these practices and aims to be as transparent as
possible on our security efforts.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Project Structure](#project-structure)
   1. [Transparency](#transparency)
      1. [Open Source](#open-source)
      1. [Workflow](#workflow)
   1. [Version Control](#version-control)
      1. [Git](#git)
      1. [Signed Commits](#signed-commits)
      1. [Protected Branches](#protected-branches)
1. [Personnel](#personnel)
   1. [Education](#education)
   1. [Policies](#policies)
   1. [Two-factor Authentication](#two-factor-authentication)
   1. [Privilege Model](#privilege-model)
   1. [Third-Parties](#third-parties)
1. [Development & Code](#development--code)
   1. [Design & Architecture](#design--architecture)
      1. [Rust](#rust)
      1. [Unsafe Code](#unsafe-code)
      1. [User Privileges](#user-privileges)
   1. [Dependencies](#dependencies)
   1. [Change Control](#change-control)
      1. [Pull Requests](#pull-requests)
      1. [Reviews & Approvals](#reviews--approvals)
      1. [Merge Policies](#merge-policies)
   1. [Automated Checks](#automated-checks)
      1. [Vulnerability Scans & Security Advisories](#vulnerability-scans--security-advisories)
      1. [Vulnerability Remediation](#vulnerability-remediation)
      1. [Fuzz Testing](#fuzz-testing)
1. [Infrastructure](#infrastructure)
   1. [CI/CD](#cicd)
      1. [Runtime Isolation](#runtime-isolation)
   1. [Network Security](#network-security)
      1. [Penetration Testing](#penetration-testing)
      1. [Protocols](#protocols)
   1. [Release Artifacts & Channels](#release-artifacts--channels)
      1. [Asset Audit Logging](#asset-audit-logging)
      1. [Asset Signatures & Checksums](#asset-signatures--checksums)
1. [Meta](#meta)
   1. [Review Schedule](#review-schedule)
   1. [Vulnerability Reporting](#vulnerability-reporting)

<!-- /MarkdownTOC -->

## Project Structure

Project structure plays an important role in security. It creates guardrails
that prevent common security issues. This section will outline our deliberate
structural decisions that impact security.

### Transparency

We believe transparency is a strong deterrent of nefarious behavior that could
otherwise undermine security.

#### Open Source

Vector and its dependencies are open-source. All code and changes are publicly
available at [our Github repo][urls.vector_repo]. While the transparent nature
open source helps to improve security, so does the large collaborative
community behind Vector.

#### Workflow

All of Vector's workflow is transparent.
[Pull requests][urls.vector_pull_requests], [issues][urls.vector_issues],
[chats][urls.vector_chat], and [our roadmap][urls.vector_roadmap]
are all publicly available.

### Version Control

Version control ensures that all code changes are audited and authentic.

#### Git

Vector leverages the [Git][urls.git] version-control system. This ensures all
changes are audited and traceable.

#### Signed Commits

Because of Vector's [merge style](CONTRIBUTING.md#merge-style), commits to
release branches are signed by Github itself during the squash and merge
process. Commits to development branches are encouraged to be signed but not
required since changes must go through a [review process](#reviews--approvals).

#### Protected Branches

Vector cuts releases from the `master` and `v*` branches _only_. These branches
are [protected][urls.github_protected_branches]. The exact requirements are:

- Cannot be deleted.
- Force pushes are not allowed.
- A linear history is required.
- Signed commits are required.
- Administrators are included in these checks.

## Personnel

### Education

Vector team members are required to review this security document as well as
the [contributing](CONTRIBUTING.md) and [reviewing](REVIEWING.md) documents.

### Policies

Vector maintains this security policy. Changed are communicated to all Vector
team members.

### Two-factor Authentication

All Vector team members are required to enable two-factor authentication
for their Github accounts.

### Privilege Model

Vector follows the [principle of least privilege][urls.least_privilege] for
its personnel access model. Vector maintains tiers user groups with tiered
privileges to ensure users only have access to the minimal resources necessary.

### Third-Parties

When used, third-parties must also adhere to this security policy. Access is
based on the [principle of least privilege][urls.least_privilege] and removed
when the contract ends.

## Development & Code

### Design & Architecture

The base of Vector's security lies in our choice of underlying technology and
decisions around design and architecture.

#### Rust

The [Rust programming language][urls.rust] is memory and thread-safe; it will
catch many common sources of vulnerabilities at compile time.

#### Unsafe Code

Vector does not allow the use of unsafe code except in circumstances where it
is required, such as dealing with CFFI.

#### User Privileges

Vector is always designed to run under non-`root` privileges, and our
documentation always defaults to non-`root` use.

### Dependencies

Vector aims to reduce the number of dependencies it relies on. If a dependency
is added it goes through a comprehensive review process that is detailed in
the [Reviewing guide](REVIEWING.md#dependencies).

### Change Control

As noted above Vector uses the Git version control system on Github.

#### Pull Requests

All changes to Vector must go through a pull request review process.

#### Reviews & Approvals

All pull requests must be reviewed by at least one Vector team member. The
review process takes into account many factors, all of which are detailed in
our [Reviewing guide](REVIEWING.md). In exceptional circumstances, this
approval can be retroactive.

#### Merge Policies

Vector requires pull requests to pass all [automated checks](#automated-checks).
Once passed, the pull request must be squashed and merged. This creates a clean
linear history with a Vector team member's co-sign.

### Automated Checks

When possible, we'll create automated checks to enforce security policies.

#### Vulnerability Scans & Security Advisories

- Vector implements an automated [`cargo deny` check][urls.cargo_deny]. This
  is part of the [Rust Security advisory database][urls.rust_sec]. The configuration, and a
  list of currently accepted advisories, are maintained in the
  [Cargo Deny configuration][urls.cargo_deny_configuration]. The check is run
  [on every incoming PR][urls.cargo_deny_schedule] to the Vector project.
- Vector implements [Dependabot][urls.dependabot] which performs automated
  upgrades on dependencies and [alerts][urls.dependabot_alerts] about any
  dependency-related security vulernerabilities.
- We verify the security of our Docker images using [Synk's][urls.snyk] vulnerability
  scanning.

#### Vulnerability Remediation

If the advisory check fails then the PR will not be merged. We review each advisory to
determine what action to take. If possible, we update the dependency to a version
where the vulnerability has been addressed. If this isn't possible we either record
the acceptance of the vulnerability or replace the dependency. If we accept the
vulnerability we open a ticket to track its remediation, generally awaiting a fix
upstream. If the risk is deemed unacceptable we revisit the code and dependency
to find a more secure alternative.

#### Fuzz Testing

Vector implements automated fuzz testing to probe our code for other sources
of potential vulnerabilities.

## Infrastructure

Because Vector is an open-source project designed to be self-hosted it uses
minimal infrastructure. Below we cover the various responsibilities for
Vector's infrastructure and how we secure them.

### CI/CD

#### Runtime Isolation

All builds run in an isolated sandbox that is destroyed after each use.

### Network Security

#### Penetration Testing

Vector performs quarterly pen tests on timber.io and vector.dev.

#### Protocols

All network traffic is secured via TLS and SSH. This includes checking out
Vector's code from the relevant [protected branch](#protected-branches),
Docker image retrieval, and publishing of Vector's release artifacts.

### Release Artifacts & Channels

#### Asset Audit Logging

Changes to Vector's assets are logged through S3's audit logging feature.

#### Asset Signatures & Checksums

All assets are signed with checksums allowing users to verify asset authenticity
upon download. This verifies that assets have not been modified at rest.

## Meta

### Review Schedule

Vector reviews this policy and all user access levels on a quarterly basis.

### Vulnerability Reporting

We deeply appreciate any effort to discover and disclose security
vulnerabilities responsibly.

If you would like to report a vulnerability or have any security concerns with
Vector, please e-mail vector@timber.io.

For non-critical matters, we prefer users [open an issue][urls.new_security_report].
For us to best investigate your request, please include any of the
following when reporting:

- Proof of concept
- Any tools, including versions used
- Any relevant output

We take all disclosures very seriously and will do our best to rapidly respond
and verify the vulnerability before taking the necessary steps to fix it. After
our initial reply to your disclosure, which should be directly after receiving
it, we will periodically update you with the status of the fix.

[urls.cargo_deny]: https://github.com/EmbarkStudios/cargo-deny
[urls.cargo_deny_configuration]: https://github.com/timberio/vector/blob/master/deny.toml
[urls.cargo_deny_schedule]: https://github.com/timberio/vector/blob/master/.github/workflows/test.yml#L267
[urls.dependabot]: https://github.com/marketplace/dependabot-preview
[urls.dependabot_alerts]: https://github.com/timberio/vector/network/alerts
[urls.git]: https://git-scm.com/
[urls.github_protected_branches]: https://help.github.com/en/github/administering-a-repository/about-protected-branches
[urls.least_privilege]: https://en.wikipedia.org/wiki/Principle_of_least_privilege
[urls.new_security_report]: https://github.com/timberio/vector/issues/new?labels=domain%3A+security
[urls.rust]: https://www.rust-lang.org/
[urls.rust_sec]: https://rustsec.org/
[urls.snyk]: https://www.snyk.io
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_issues]: https://github.com/timberio/vector/issues
[urls.vector_pull_requests]: https://github.com/timberio/vector/pulls
[urls.vector_repo]: https://github.com/timberio/vector
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
