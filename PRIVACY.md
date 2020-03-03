# Privacy Policy

It should go without saying, but Vector takes the privacy of your data,
including how you use Vector, very seriously. This document clarifies how the
Vector project thinks about privacy now and in the future.

<!-- MarkdownTOC autolink="true" style="ordered" -->

1. [Vector Itself](#vector-itself)
  1. [Downloads](#downloads)
  1. [Phoning Home](#phoning-home)
1. [Vector Website & Docs](#vector-website--docs)
1. [Vector Community](#vector-community)

<!-- /MarkdownTOC -->

## Vector Itself

### Downloads

Vector uses [Amaazon S3][], [Github assets][], and [Docker Hub][] to host
release artifacts. Vector does track download counts in aggregate. For Github
and Docker this data is anonymous, but for S3 IP addresses are logged. There
is no way to disable IP address tracking within the S3 logs. If you are
concerned about sharing your IP address we recommend using a proxy or
downloading Vector from a different channel.

### Phoning Home

Vector, under no circumstances, will "phone home" and communicate with an
external service that you did not explicitly configure as part of setting up
Vector. This includes grey area tactices such as version checks, sharing
diagnostic information, sharing crash reports, etc.

## Vector Website & Docs

The Vector website does not implement any front-end trackers. Aggregated
analytics data is derived from backend server logs which are anonymized.

## Vector Community

The Vector community largely depends on hosted services. Each hosted service
and it's privacy policy is below:

* Github - [Privacy Policy][github_pp]
* Gitter - [Privacy Policy][gitter_pp]

github_pp: https://help.github.com/en/github/site-policy/github-privacy-statement