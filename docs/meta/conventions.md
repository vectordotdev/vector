---
description: Vector documentation conventions
---

# Conventions

The Vector documentation uses the following conventions. It's important all
documenters follow a consistent style to producive cohesive documentation.
Documentation is _very_ important to Vector since it is a significant piece
of the user experience.

The sections are ordered by generality and designed to be progressive.
General section at the top, specific sections at the bottom.

## Syntax

### Lists

When specifying option types, if the type is enclosed with `[ ]` symbols then
this denotes a list or array. For example:

| Name | Type | Description |
| :--- | :--- | :--- |
| `option_name` | `[string]` | Option description. |

`[string]` in the above example is an array, or list, of strings.

### Variables

#### Required Variables

Within code samples, if word is enclosed with `< >` symbols, this is a variable
and it is required. For example:

```coffeescript
[sinks.<sink-id>]
    type = "s3"
```

The entire `<sink-id>` variable must be replaced.

#### Optional Variables

Within code sample, if a word is enclosed with `[< >]` symbols, this is a
variable and it is optional. For example:

```text
vector --debug [<sink-id>]
```

The entire `[<sink-id>]` variable is optional.

#### Enumeration

Enumerations represent a finite list of acceptable values that are represented
with the following syntax:

```text
{"value1" | "value2" }
```

This can be extended entire variables:

```coffeescript
inputs = ["{<source-id> | <transform-id>}"]
```

In this case, the `<source-id>` or `<transform-id>` must be supplied.

## Style

### Configuration Examples

All [sources][docs.sources], [transforms][docs.transforms], and
[sinks][docs.sinks] must include comprehensive configuration examples. This
means all options must be represented. The example should be formatted as
follows:

```coffeescript
[<type>.<id>]
# REQUIRED - General
inputs = ["{<source-id> | <transform-id>}"] # not relevant for sources
type = "<type>"
<key> = <value>

# OPTIONAL - General
<key> = <value>

# OPTIONAL - <category>
[<type>.<id>.<table-key>]
<key> = <value>
```

* Options should be grouped into sections.
* Options should be sorted alphabetically unless options being at the top are
  descriptive. Ex: the `inputs` and `type` options should be at the top.
* Required sections must be at the top.
* Tables must be at the bottom since this would otherwise be invalid TOML.

### Document

* Lines should not exceed 80 characters in width.
* All documents should have an H1 heading.

### Headings

* H1s and H2s should titleize every word. Ex: `This Is A Title`
* H3s should titleize only the first word. Ex: `This is a title`

### Images

Images are preferred in the SVG format since it is scalable. If SVG is not
possible then PNG is preferred.

Image source files, such as templates, should be included in the
`assets/source` directory. If possible, you should use these source files to
create diagrams and images as it keeps a consistent theme.

### JSON

JSON documents should be presented in `javascript` code format \(since our
documentation system does not have a JSON format\). `"..."` should be used to
represent variable string values.

### Links

Avoid saying "click here", instead, you should turn the relevant word\(s\)
into a link. For example:

* **Good:** See the [How It Works](conventions.md#links) section.
* **Bad:** Click [here](conventions.md#links) to learn more.

### Options

When displaying options, a table must adhere to the following format:

<table>
  <thead>
    <tr>
      <th style="text-align:left">Name</th>
      <th style="text-align:left">Type</th>
      <th style="text-align:left">Description</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td style="text-align:left"><b>Required</b>
      </td>
      <td style="text-align:left"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>name</code>
      </td>
      <td style="text-align:left"><code>type</code>
      </td>
      <td style="text-align:left">
        <p>Description. See <a href="conventions.md#displaying-options">Section</a> for
          more info.</p>
        <p><code>default: &quot;value&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>Optional</b>
      </td>
      <td style="text-align:left"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>name</code>
      </td>
      <td style="text-align:left"><code>type</code>
      </td>
      <td style="text-align:left">
        <p>Description. See <a href="conventions.md#displaying-options">Section</a> for
          more info.</p>
        <p><code>no default</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>Where:

* `name` is the name of the option.
* `type` is one of the supported types, typically a [TOML type](https://github.com/toml-lang/toml#table-of-contents).
* The description _succinctly_ describes what the variable does.
* A more in-depth description should be moved to a separate section and linked.
* Default should be specified on a new line if relevant.
* `no default` should be used if it is not already obviously implied.

### Sources, Transforms, & Sinks

#### Heading

The heading must include the appropriate diagram highlighting the component
with a succinct description of what the component does. It should mention the
event types that it accepts.

#### Section Hierarchy

Source, transform, and sink pages must be structured to include the following
section hierarchy. The root level represents an `h1`, children are `h2`, and
so on:

* **Configuration File** - A configuration example that includes all options, [formatted
  appropriately](conventions.md#configuration-examples).
* **Options** - A table representing the available options, [formatted
  appropriately](conventions.md#options).
* **Examples** - The data type accepted as input, must link to the appropriate type
  in the [Data Model document](../about/data-model.md).
* **How It Works**
  * **Context** - Any keys added to the event that represent context \(such as
    `"host"`\).
  * **Guarantees** - The [guarantee](../about/guarantees.md) a source or sink
    can achieve.
* **Resources** - A list of linked resources, such as source code, issues, and
  so on.

## Organization

Vectors documentation is organized in a specific manner. Outside of the obvious
sections defined in the [SUMMARY.md][docs.summary], there are logical rules that
dictate where a document should be placed. The following sections describe those
rules.

### Guides

Within the "Usage" category is a [Guides section][docs.guides]. Guides are not
replacements for documentation on the topic they are covering, they are
supplemental tutorials or walkthroughs. For example, monitoring is covered
under the [Administration section][docs.administration], but we should also
offer a monitorig guide that provides a full walk through with specific
integrations.

## Language

### Factual Tone

Vector's documentation tone is factual and academic. Convey subject matter in a
clear, concise, and confident manner.

Avoid using vague language such as “it seems” or “probably.” Instead of:

> It seems like every SSL reseller packs their certs in a slightly different
way with slightly different filenames.

Use:

> SSL resellers use a variety of naming conventions when packaging certs.

### Second Person Narrative

The [second-person point of
view](https://wikipedia.org/wiki/Narration#Second-person) uses "you" to address
the reader. It works well in technical documentation because it focuses on the
reader and enables you to use the imperative mood. Avoid using “I” or “we” \(the
[first-person point of
view](https://en.wikipedia.org/wiki/Narration#First-person)\).

Instead of relating your personal experiences to the reader:

> Based on our own experience managing remote assets, we created the foo gem so
> you can transparently upload your static assets to S3 on deploy.

Present concepts based on their own merits:

> The foo gem enables you to upload static assets transparently at deploy time.

### Present Tense

Use the present tense whenever possible. Phrases such as "was created" indicate
unnecessary use of the past tense. Instead of:

> This guide was created to describe the characteristics of a well-written
> Vector article.

Use:

> This guide describes the characteristics of a well-written Vector article.

Similarly, avoid unnecessary use of the future tense. Instead of:

> After you log in, the registration dialog will appear.

Use:

> After you log in, the registration dialog appears.

## Best Practices

### Link

* Link to other internal documents when possible. You can do this by
  highlighting the word and using `ctrl+k` to search for and link to a document.
* If in the same section you only need to link the first occurrence of the word,
  do not link every single occurrence.
* When linking to documents, try to link to the specific section.

### Shallow Scope

When writing a document put yourself in the shoes of a user coming from a search
engine and landing on that page for the first time. They do not have any
preconceived knowledge of Vector; they do not know Vector's terms, patterns, or
rules. Because of this, documentation should be shallow, explicit, and clear.
Users should not have to jump around to obtain the full scope of a document. If
a document does require advanced knowledge of another topic you should preface
the document with that, and link to the document covering that topic.

Here a few examples to help illustrate this point:

* Every [source](../usage/configuration/sources/),
  [transform](../usage/configuration/transforms/), and
  [sink](../usage/configuration/sinks/) includes _all_ options, even if they are
  foundational options that are shared and repeated across all components. This
  avoids the need for a user to have to jump around to separate pages to get the
  full scope of options available to them.
* All of the `aws_*` sources and sinks include an "Authentication" section that
  repeats the same language. This is easier for the user since it is contained
  in the relevant integration page. The user should not have to jump to a
  separate "AWS Authentication" page unless this subject deserves it's own
  entire document. Even then, each `aws_*` source and sink should include a link
  to that document.


[docs.administration]: ../usage/administration
[docs.guides]: ../usage/guides
[docs.sinks]: ../usage/configuration/sinks
[docs.sources]: ../usage/configuration/sources
[docs.summary]: ../SUMMARY.md
[docs.transforms]: ../usage/configuration/transforms
