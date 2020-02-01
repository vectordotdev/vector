# Security Policy

---

<p align="center">
  <strong>Reporting a vulnerability? See the ["Vulnerability Reporting" section](#vulnerability-reporting)</strong>
</p>

---

We understand that many users place a high level of trust in Vector to collect
and ship mission-critical data. The security of Vector is a top priority.
That's why we apply widely accepted best practices when it comes to security.
This document will describe these practices and aims to be as transparent as
possible on our security efforts.

<!-- MarkdownTOC -->

- Project Structure
  - Transparency
    - Open Source
    - Workflow
  - Version Control
    - Git
    - Signed Commits
    - Protected Branches
- Personnel
  - Background Checks
  - Education
  - Policies
  - Two-factor Authentication
- Development
  - Design & Architecture
    - Rust
    - Unsafe Code
    - User Privileges
  - Dependencies
  - Change Control
    - Pull Requests
    - Reviews & Approvals
    - Merge Policies
    - Automated Checks
      - Vulnerability Scans
      - Fuzz Testing
- Building & Releasing
  - Network Security
  - Runtime Isolation
  - Asset Encryption
  - Asset Audit Logging
  - Asset Signatures & Checksums
- Vulnerability Reporting

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

All commits within Vector must be cryptographically signed. This ensures all
commits are verified, trusted, and authentic.

#### Protected Branches

Vector cuts releases from the `master` and `v*` branches _only_. These branches
are [protected][urls.github_protected_branches]. The exact requirements are:

* Cannot be deleted.
* Force pushes are not allowed.
* A linear history is required.
* Signed commits are required.
* Administrators are included in these checks.

## Personnel

### Background Checks

All Vector team members undergo background checks before access is permitted.

### Education

Vector team members are required to review this security document as well as
the [contributing](CONTRIBUTING.md) and [reviewing](REVIEWING.md) documents.

### Policies

Vector maintains this security policy. Changed are communicated to all Vector
team members.

### Two-factor Authentication

All Vector team members are required to enable two-factor authentication
on their work machines as well as their Github accounts.

## Development

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

Vector is always designed to run under non-`sudo` privileges, and our
documentation always defaults to non-`sudo` use.

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

#### Automated Checks

When possible, we'll create automated checks to enforce security policies.

##### Vulnerability Scans

Vector implements an automated [`cargo audit` check][urls.cargo_audit]. This
is part of the [Rust Security advisory database][urls.rust_sec].

##### Fuzz Testing

Vector implements automated fuzz testing to probe our code for other sources
of potential vulnerabilities.

## Building & Releasing

Vector takes care to secure the build and release process to prevent unintended
modifications.

### Network Security

All network traffic is secured via TLS and SSH. This includes checking out
Vector's code from the relevant [protected branch](#protected-branches),
Docker image retrieval, and publishment of Vector's release artifacts.

### Runtime Isolation

All builds run in an isolated sandbox that is destroyed after each use.

### Asset Encryption

Vector's artifacts are stored on S3 with SSE encryption enabled.

### Asset Audit Logging

Changes to Vector's assets are logged through S3's audit logging feature.

### Asset Signatures & Checksums

All assets are signed with checksums allowing users to verify asset authenticity
upon download. This verifies that assets have not been modified at rest.

## Vulnerability Reporting

We deeply appreciate any effort to discover and disclose security
vulnerabilities responsibly.

If you would like to report a vulnerability or have any security concerns with
Vector, please e-mail vector@timber.io.

For non-critical matters, we prefer users [open an issue][urls.new_security_report].
For us to best investigate your request, please include any of the
following when reporting:

* Proof of concept
* Any tools, including versions used
* Any relevant output

We take all disclosures very seriously and will do our best to rapidly respond
and verify the vulnerability before taking the necessary steps to fix it. After
our initial reply to your disclosure, which should be directly after receiving
it, we will periodically update you with the status of the fix.


[urls.cargo_audit]: https://github.com/RustSec/cargo-audit
[urls.git]: https://git-scm.com/
[urls.github_protected_branches]: https://help.github.com/en/github/administering-a-repository/about-protected-branches
[urls.new_security_report]: https://github.com/timberio/vector/issues/new?labels=domain%3A+security
[urls.rust]: https://www.rust-lang.org/
[urls.rust_sec]: https://rustsec.org/
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_issues]: https://github.com/timberio/vector/issues
[urls.vector_pull_requests]: https://github.com/timberio/vector/pulls
[urls.vector_repo]: https://github.com/timberio/vector
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
