# Security Policy

We understand that many users place a high level of trust in Vector to collect
and ship mission critical data. The security of Vector is a top priority.
That's why we apply widely accepted best practices when it comes to security.

## Technical Measures

The base of Vector's security lies in our choice of underlying technology. The
Rust programming language is memory safe and will catch many common sources of
vulnerabilities at compile time. On top of the language itself, we routinely
employ security-oriented techniques like fuzz testing to probe our code for
other sources of potential vulnerabilities. To cover our dependencies, we have
tooling that automatically checks each library we depend on against a database
of known vulnerabilities. If it finds we are including a vulnerable version of
a library, we are notified so that we can evaluate the effect and take
appropriate action.

## Change Control

In addition to those technology choices, we employ a change control and release
process to secure our builds and artifact distribution. First, all code changes
go through a Pull Request process where they are approved by at least one member
of the Vector team that was not involved in authoring the change (in exceptional
circumstances, this approval can be retroactive). This helps to ensure the
integrity of the code base itself. We then use automated tooling to build and
distribute the Vector installable artifacts, ensuring they include only
authorized changes.

## Vulnerability Reporting

We deeply appreciate any effort to discover and disclose security
vulnerabilities responsibly.

If you would like to report a vulnerability, or have any security concerns with
Vector, please e-mail vector@timber.io.

For non-critical matters, we prefer users [open an issue][urls.new_security_report].
In order for us to best investigate your request, please include any of the
following when reporting:

* Proof of concept
* Any tools, including versions used
* Any relevant output

We take all disclosures very seriously and will do our best to rapidly respond
and verify the vulnerability before taking the necessary steps to fix it. After
our initial reply to your disclosure, which should be directly after receiving
it, we will periodically update you with the status of the fix.

[urls.new_security_report]: https://github.com/timberio/vector/issues/new?labels=domain%3A+security
