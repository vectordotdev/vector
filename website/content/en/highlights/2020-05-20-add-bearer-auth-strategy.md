---
date: "2020-07-13"
title: "Now supporting the bearer auth strategy"
description: "Vector can now bear authentication tokens for relevant components."
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2607]
release: "0.10.0"
badges:
  type: "enhancement"
  domains: ["sinks"]
  sinks: ["http"]
---

The light reading material of [IETF RFC 6750][urls.ietf_rfc_6750] taught us all about how bearer auth works, right?

You glazed over it? Fine. We read it (and implemented it!) for you. Now you can have Vector use bearer tokens with your favorite (and not so favorite) services.

Just drop your token in, and you're done.

```diff title="vector.toml"
 [sinks.example]
   type = "http"
+  auth.strategy = "bearer"
+  auth.token = "B14CK-L1V35-M4TT4R"
```

**Reminder:** If you're building a [12 Factor App][urls.twelve_factor_app] you may wish to use environment variables!

```diff title="vector.toml"
 [sinks.example]
   type = "http"
+  auth.strategy = "bearer"
+  auth.token = "${VECTOR_SINKS_HTTP_example_AUTH_TOKEN}"
```

[Check out the HTTP sink token docs][urls.vector_http_auth_token] for more information.

[urls.ietf_rfc_6750]: https://tools.ietf.org/html/rfc6750
[urls.twelve_factor_app]: https://12factor.net/
[urls.vector_http_auth_token]: /docs/reference/configuration/sinks/http/#auth.token
