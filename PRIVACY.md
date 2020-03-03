# Privacy Policy

It should go without saying, but Vector takes the privacy of your data,
including how you use Vector, very seriously. Vector is used to collect and
route some of your most sensitive data and we want you to know that we do not
take that lightly. This document clarifies how the Vector project thinks about
privacy now and in the future.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Vector Itself](#vector-itself)
   1. [Downloads](#downloads)
   1. [Phoning Home](#phoning-home)
1. [Vector Website & Docs](#vector-website--docs)
1. [Vector Community](#vector-community)
   1. [Vector Repository](#vector-repository)
   1. [Vector Chat](#vector-chat)
   1. [Vecotr Mailinglist](#vecotr-mailinglist)

<!-- /MarkdownTOC -->

## Vector Itself

### Downloads

Vector uses Amaazon S3, Github assets, and Docker Hub to host release artifacts.
Vector does track download counts in aggregate. For Github and Docker this data
is anonymous, but for S3 IP addresses are logged. There is no way to disable IP
address tracking within the S3 logs. If you are concerned about sharing your IP
address we recommend using a proxy or downloading Vector from a different
channel.

### Phoning Home

Vector, under no circumstances, now and in the future, will "phone home" and
communicate with an external service that you did not explicitly configure as
part of setting up Vector. This includes grey-area tactics such as version
checks, capturing diagnostic information, and sharing crash reports.

## Vector Website & Docs

The Vector website does not implement any front-end trackers. Aggregated
analytics data is derived from backend server logs which are anonymized.
Vector uses [Netlify analytics][netlify_analytics] for this.

## Vector Community

### Vector Repository

The Vector repository is hosted on Github. You can review their privacy policy
[here][github_pp]. Additionally, Vector will not attempt to mine information
about users that interact with Vector on Github. Vector team members will
occassionaly reach out to active users offer help debugging or learn about
ways Vector can improve.

### Vector Chat

The Vector chat uses Gitter, which is owned by Gitlab; you can review their
privacy policy [here][gitter_pp].

### Vecotr Mailinglist

The Vector mailing list uses Vero; you can review their privacy policy
[here][vero_pp]. Additionally, Vector will never share your email with 3rd party
for any reason, and Vector will not send you spam email.

[github_pp]: https://help.github.com/en/github/site-policy/github-privacy-statement
[gitter_pp]: https://about.gitlab.com/privacy/
[netlify_analytics]: https://www.netlify.com/products/analytics/
[vero_pp]: https://www.getvero.com/privacy/
