# Support

Vector is a growing open source project with a variety of people willing to help
you through various mediums.

Take a look at those mediums listed at <https://vector.dev/community>

## How to ask a question about Vector

Whether in our community [Discord](https://chat.vector.dev/) server or [GitHub
Discussions](https://github.com/vectordotdev/vector/discussions), framing your
question well and providing the right level of details is essential to getting
your question answered.

Please follow the below guidelines when posting a question:

### Before posting

#### Search for your question

Use the Discord or GitHub search bar, to search for keywords related to your
issue. It is quite possible someone has already asked your question before.

This is especially useful if you have a specific error message you observing.

#### Check the docs

<https://vector.dev/docs/>

### Context and framing

#### Provide details

Essential details to include:

- What is the version of Vector (and the Helm chart if deploying via Helm) and
  the versions of any other systems in use (like Elasticsearch, NATs, etc.).
- What is your Vector configuration. See the below section on [how to format
  your config](#formatting).
- How are you [deploying](https://vector.dev/docs/setup/deployment/) Vector?
- What is your complete deployment architecture? For example: I have Logstash
  agents sending to Vector over syslog that is being forwarded to Loki.

Situation specific (not exhaustive):

- Did it occur after upgrading to a new version of Vector?
- Are you trying out Vector for the first time, or did you have a previous
  working configuration?
- Is this the first time you are using
  [VRL](https://vector.dev/docs/reference/vrl/#learn)? If so, also try out the
  [VRL playground](https://playground.vrl.dev/) to debug

These are just some examples of questions that may or may not apply to your
situation.

The more specifics you can provide, the more data we have to work with and the
more likely we'll be able to help.

#### Actually ask a question

Hopefully it is obvious, but in case not if an error message is all that is
provide in your post, there is not much we can do to help you.

### Formatting

When providing snippets of configuration files, or log messages, **_please_**
format these with backticks.

This small effort (a few extra keystrokes) renders the text into a monospaced
block which greatly improves the readability of your config, errors, and post in
general.

Formatting this way also can reveal configuration errors that you might have
missed, and _not_ formatting your config can lead to those attempting to answer
your question incorrectly reading it due to whitespace discrepancies.

#### Blocks

Blocks should be formatted with three backticks (\`\`\`)

This should be used for any configuration snippets of Vector, Helm values, etc.
and for Vector console log messages.

If you are not aware of how to format text this way, it is explained in the
[Markdown documentation on fenced code
blocks](https://www.markdownguide.org/extended-syntax/#fenced-code-blocks).

For example (wrong way):

[sinks.sink0]

inputs = ["source0"]

target = "stdout"

type = "console"

vs (right way):

```toml [sinks.sink0] inputs = ["source0"] target = "stdout" type = "console"
```

#### Single word or phrase

Formatting things like component names, versions etc. is done with single
backticks (\`) around the word or phrase.

This is less critical but also helps readability a lot and is greatly
appreciated.

See the [markdown documentation
(#code)](https://www.markdownguide.org/basic-syntax/#code) for an explanation.

For example:

"We upgraded from v24 to 25.1 and are seeing the following error output from
kafka."

vs

"We upgraded from `v0.24.0` to `v0.25.1` and are seeing the following error
output from the `kafka` sink."
