# Documenting

This document covers the basics of writing documentation within Vector.
In this document:

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Prerequisites](#prerequisites)
1. [How It Works](#how-it-works)
1. [Making Changes](#making-changes)
1. [Conventions](#conventions)
   1. [Syntax](#syntax)
      1. [Lists](#lists)
      1. [Required Variable](#required-variable)
      1. [Optional Variable](#optional-variable)
      1. [At Least One Variable Required](#at-least-one-variable-required)
   1. [Style](#style)
      1. [Configuration Examples](#configuration-examples)
      1. [Headings](#headings)
      1. [Images](#images)
      1. [JSON](#json)
      1. [Links](#links)
      1. [Options](#options)
   1. [Structure](#structure)
      1. [Guides](#guides)
      1. [Sources, Transforms, & Sinks](#sources-transforms--sinks)
         1. [Heading](#heading)
         1. [Section Hierarchy](#section-hierarchy)
   1. [Language](#language)
      1. [Factual Tone](#factual-tone)
      1. [Second Person Narrative](#second-person-narrative)
      1. [Present Tense](#present-tense)
   1. [Best Practices](#best-practices)
      1. [Link](#link)
      1. [Shallow Scope](#shallow-scope)

<!-- /MarkdownTOC -->


## Prerequisites

1. **You are familiar with the [docs](https://docs.vector.dev).**
2. **You have read the [Contributing](/CONTRIBUTING.md) guide.**
3. **You understand [markdown](https://daringfireball.net/projects/markdown/).**

## How It Works

1. Vector's documentation is located in the [/docs](/docs) folder.
2. All files are in markdown format.
3. The documentation is a mix of hand-written and generated docs.
4. Docs are generated via the `make generate-docs` command which delegates to
   the [`scripts/generate_docs.sh`](/scripts/generate_docs.sh) file.
   1. This is a mix of Ruby scripts that parses the [`scripts/metadata.toml`]
      file and runs a series of generators.
   2. Each generated section clearly called out in the markdown file to ensure
      humans do not modify it:

      ```
      <!-- START: sources_table -->
      <!-- ----------------------------------------------------------------- -->
      <!-- DO NOT MODIFY! This section is generated via `make generate-docs` -->

      ...

      <!-- ----------------------------------------------------------------- -->
      <!-- END: sources_table -->
      ```

## Making Changes

You can edit the markdown files directly in the /docs folder.  Auto-generated
sections are clearly denoted as described above. To make make changes
to aut-generated sections:

1. Modify the `scripts/metadata.toml` file as necessary.
2. Run `make generate-docs`
3. Commit changes.

## Conventions

The Vector documentation uses the following conventions.

### Syntax

#### Lists

When specifying option types, if the type is enclosed with `[ ]` symbols then this denotes a list or array. For example:

| Name | Type | Description |
| :--- | :--- | :--- |
| `option_name` | `[string]` | Option description. |

`[string]` in the above example is an array, or list, of strings.

#### Required Variable

Within code samples, if word is enclosed with `< >` symbols, this is a variable and it is required. For example:

```toml
[sinks.<sink-id>]
    type = "s3"
```

The entire `<sink-id>` variable must be replaced.

#### Optional Variable

Within code sample, if a word is enclosed with `[< >]` symbols, this is a variable and it is optional. For example:

```text
vector --debug [<sink-id>]
```

The entire `[<sink-id>]` variable is optional.

#### At Least One Variable Required

Within code samples, if a word is enclosed with `{ }` symbols, then at least one of the variables listed is required. For example:

```toml
inputs = ["{<source-id> | <transform-id>}"]
```

Either `<source-id>` or `<transform-id>` must be supplied. The enclosing `{ }` should be removed.

### Style

#### Configuration Examples

All sources, transforms, and sinks must include comprehensive configuration examples. This means all options must be represented. The example should be formatted as follows:

```toml
# REQUIRED
inputs = ["{<source-id> | <transform-id>}"] # not relevant for sources
type = "<type>"
<key> = <value>

# OPTIONAL
<key> = <value>
```

Options should be sorted alphabetically within each section, with the exception of `inputs` and `type` at the top.

#### Headings

* H1s and H2s should titleize every word. Ex: `This Is A Title`
* H3s should titleize only the first word. Ex: `This is a title`

#### Images

Images are preferred in the SVG format since it is scalable. If SVG is not possible then PNG is preferred.

Image source files, such as templates, should be included in the `assets/source` directory. If possible, you should use these source files to create diagrams and images as it keeps a consistent theme.

#### JSON

JSON documents should be presented in `javascript` code format \(since our documentation system does not have a JSON format\). `"..."` should be used to represent variable string values.

#### Links

Avoid saying "click here", instead, you should turn the relevant word\(s\) into a link. For example:

* **Good:** See the [How It Works](conventions.md#links) section.
* **Bad:** Click [here](conventions.md#links) to learn more.

#### Options

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

### Structure

#### Guides

Within the "Usage" category is a [Guides section][docs.guides]. Guides are not
replacements for documentation on the topic they are covering, they are
supplemental tutorials or walkthroughs. For example, monitoring is covered
under the [Administration section][docs.administration], but we should also
offer a monitorig guide that provides a full walk through with specific
integrations.

#### Sources, Transforms, & Sinks

##### Heading

The heading must include the appropriate diagram highlighting the component
with a succinct description of what the component does. It should mention the
event types that it accepts.

##### Section Hierarchy

Source, transform, and sink pages must be structured to include the following
section hierarchy. The root level represents an `h1`, children are `h2`, and
so on:

* **Example** - A configuration example that includes all options, [formatted appropriately](conventions.md#configuration-examples).
* **Options** - A table representing the available options, [formatted appropriately](conventions.md#options).
* **Input** - The data type accepted as input, must link to the appropriate type in the [Data Model document](../about/data-model.md).
* **Output** - The data type that is output, must link to the appropriate type in the [Data Model document](../about/data-model.md).
* **How It Works**
  * **Context** - Any keys added to the event that represent context \(such as  `"host"`\).
  * **Guarantees** - The [guarantee](../about/guarantees.md) a source or sink can achieve.
* **Resources** - A list of linked resources, such as source code, issues, and so on.

### Language

#### Factual Tone

Vector's documentation tone is factual and academic. Convey subject matter in a
clear, concise, and confident manner.

Avoid using vague language such as “it seems” or “probably.” Instead of:

> It seems like every SSL reseller packs their certs in a slightly different
way with slightly different filenames.

Use:

> SSL resellers use a variety of naming conventions when packaging certs.

#### Second Person Narrative

The [second-person point of view](https://wikipedia.org/wiki/Narration#Second-person) uses "you" to address the reader. It works well in technical documentation because it focuses on the reader and enables you to use the imperative mood. Avoid using “I” or “we” \(the [first-person point of view](https://en.wikipedia.org/wiki/Narration#First-person)\).

Instead of relating your personal experiences to the reader:

> Based on our own experience managing remote assets, we created the foo gem so you can transparently upload your static assets to S3 on deploy.

Present concepts based on their own merits:

> The foo gem enables you to upload static assets transparently at deploy time.

#### Present Tense

Use the present tense whenever possible. Phrases such as "was created" indicate unnecessary use of the past tense. Instead of:

> This guide was created to describe the characteristics of a well-written Vector article.

Use:

> This guide describes the characteristics of a well-written Vector article.

Similarly, avoid unnecessary use of the future tense. Instead of:

> After you log in, the registration dialog will appear.

Use:

> After you log in, the registration dialog appears.

### Best Practices

#### Link

* Link to other internal documents when possible. You can do this by highlighting the word and using `ctrl+k` to search for and link to a document.
* If in the same section you only need to link the first occurrence of the word, do not link every single occurrence. 
* When linking to documents, try to link to the specific section.

#### Shallow Scope

When writing a document put yourself in the shoes of a user coming from a search engine and landing on that page for the first time. They do not have any preconceived knowledge of Vector; they do not know Vector's terms, patterns, or rules. Because of this, documentation should be shallow, explicit, and clear. Users should not have to jump around to obtain the full scope of a document. If a document does require advanced knowledge of another topic you should preface the document with that, and link to the document covering that topic.

Here a few examples to help illustrate this point:

* Every [source](../usage/configuration/sources/), [transform](../usage/configuration/transforms/), and [sink](../usage/configuration/sinks/) includes _all_ options, even if they are foundational options that are shared and repeated across all components. This avoids the need for a user to have to jump around to separate pages to get the full scope of options available to them.
* All of the `aws_*` sources and sinks include an "Authentication" section that repeats the same language. This is easier for the user since it is contained in the relevant integration page. The user should not have to jump to a separate "AWS Authentication" page unless this subject deserved it's own entire document. Even then, each `aws_*` source and sink should include a link to that document.


[docs.administration]: "../../usage/administration.md"
[docs.guides]: "../../usage/guides/"
