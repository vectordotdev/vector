# Support

Vector is a growing open source project with a variety of people willing to help
you through various mediums.

Take a look at those mediums listed at <https://vector.dev/community>

## How to ask a question about Vector

Whether in our community [Discord][discord] server or [GitHub Discussions][discussions], framing
your question well and providing the right level of details will improve your chances of getting
your question answered. Here are some tips:

### Before asking

Check the [Vector documentation](https://vector.dev/docs/) first to see if it answers your question.
If your question is about [VRL](https://vector.dev/docs/reference/vrl/#learn), you can also try out
the [VRL playground][vrl_playground].

If the docs do not answer your question, try using the search feature on [Discord][discord] or
[GitHub][vector], to search for keywords related to your issue. It is quite possible someone has
already asked your question before. This is especially useful if you have a specific error message
you are observing.

### Provide details

Essential details to include:

- Errors- for each error, please provide the full error message snippet, and
  details such as where the error is observed, at what stage in the process
  (e.g. at boot time, after some specific condition etc.).
- What is the version of Vector (and the Helm chart if deploying via Helm) and
  the versions of any other systems in use (like Elasticsearch, NATS, etc.).
- What is your Vector configuration. See the below section on [how to format
  your config](#formatting).
- How are you [deploying](https://vector.dev/docs/setup/deployment/) Vector?
- What is your complete deployment architecture? For example: I have Logstash
  agents sending to Vector over syslog that is being forwarded to Loki.

Situation specific (not exhaustive):

- Did it occur after upgrading to a new version of Vector?
- Are you trying out Vector for the first time, or did you have a previous
  working configuration?

These are just some examples of questions that may or may not apply to your
situation.

### Formatting

When providing snippets of configuration files, or log messages, format these with backticks to
improve the legibility for readers.

#### Blocks

Blocks should be formatted with three backticks (\`\`\`)

This should be used for any configuration snippets of Vector, Helm values, etc.
and for Vector console log messages.

See the [markdown documentation](https://www.markdownguide.org/basic-syntax/#fenced-code-blocks)
for an explanation.

For example:

[sinks.sink0]

inputs = ["source0"]

target = "stdout"

type = "console"

should be written as

```toml
[sinks.sink0]
inputs = ["source0"]
target = "stdout"
type = "console"
```

#### Single word or phrase

Formatting things like component names, versions etc. is done with single
backticks (\`) around the word or phrase.

This is less critical but also helps readability a lot and is greatly
appreciated.

See the [markdown documentation](https://www.markdownguide.org/basic-syntax/#code)
for an explanation.

For example:

> We upgraded from v24 to 25.1 and are seeing the following error output from
kafka.

should be written as

> We upgraded from `v0.24.0` to `v0.25.1` and are seeing the following error
output from the `kafka` sink.

[discord]: https://chat.vector.dev
[discussions]: https://github.com/vectordotdev/vector/discussions
[vector]: https://github.com/vectordotdev/vector
[vrl_playground]: https://playground.vrl.dev
