---
name: Bug
about: Let us know about an unexpected error, a crash, or an incorrect behavior.
labels: 'type: bug'
assignees: ''
title: ''

---

<!--
Hi there,

Thank you for opening an issue. Please note that we try to keep the Vector issue tracker reserved for bug reports and feature requests. For general usage questions, please see: https://chat.vector.dev.
-->

<!--- Please keep this note for the community --->

### Community Note

* Please vote on this issue by adding a 👍 [reaction](https://blog.github.com/2016-03-10-add-reactions-to-pull-requests-issues-and-comments/) to the original issue to help the community and maintainers prioritize this request
* If you are interested in working on this issue or have submitted a pull request, please leave a comment

<!--- Thank you for keeping this note for the community --->

### Vector Version

<!---
Run `vector --version` to show the version, and paste the result between the ``` marks below.

If you are not running the latest version of Vector, please try upgrading because your issue may have already been fixed.
-->

```
...
```

### Vector Configuration File

<!--
Paste the relevant parts of your `vector.toml` configuration between the ``` marks below.

!! If your config files contain sensitive information please remove it !!
-->

```toml
...
```

### Debug Output

<!--
Full debug output can be obtained by running Vector with the following:

```
RUST_BACKTRACE=full vector -vvv <rest of commands>
```

Please create a GitHub Gist containing the debug output. Please do _not_ paste the debug output in the issue, since debug output is long.

!! Debug output may contain sensitive information. Please review it before posting publicly. !!
-->


### Expected Behavior

<!--
What should have happened?
-->

### Actual Behavior

<!--
What actually happened?
-->

### Example Data

<!--
Please provide any example data that will help debug the issue, for example:

```
201.69.207.46 - kemmer6752 [07/06/2019:14:53:55 -0400] "PATCH /innovative/interfaces" 301 669
```
-->

### Additional Context

<!--
Is there anything atypical about your situation that we should know? For example: is Vector running in Kubernetes? Are you passing any unusual command line options or environment variables to opt-in to non-default behavior?
-->

### References

<!--
Are there any other GitHub issues (open or closed) or Pull Requests that should be linked here? For example:

- #6017

-->
