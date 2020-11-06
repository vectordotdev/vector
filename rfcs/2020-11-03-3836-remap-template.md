# RFC 3836 - 2020-11-03 - Remap support for template strings

We would like to unify the templating syntax within configuration fields with
the Remap language.


## Scope

- What will this RFC cover, and what will it not cover?
- Link to any previous RFCs for additional context.

This RFC will look into ways we can use Remap whilst still supporting the current
method of templating fields.


## Motivation

- What is the problem?
- What pain motivated this change?
- Do not cover benefits of your change, that should be covered in the "Rationale" section.


There are two issues that need to be resolved for us to do this.

1. It isn't that easy to automatically detect if the old template syntax or
   the Remap syntax is being used. We could parse the string with Remap first.
   If it fails to parse then we could assume it is old syntax. However, this
   means that we won't be able to capture and report any errors if the user
   intended to write a Remap template, but made an error. For example, the
   script `del(.foo)` is not valid Remap in this instance, because `del` is a
   mutable function and not allowed for template fields. So, Remap will not
   parse this and the template will assume this is just a constant string to
   use. No error will be reported.

I think the user will need to explicitly state which syntax they are using in
the configuration.

2. The Loki sink wants a list of the fields that were used in the template. It
uses this list to remove these fields from the event sent to Loki. With Remap
this list becomes dynamic. So the script `if .foo { .bar } else { .baz }` 
means sometimes .bar is used and other times .baz is used.


## Internal Proposal

- Describe your change as if you were presenting it to the Vector team.
- Use lists, examples, and code blocks for efficient reading.
- Be specific!

As there are a number of complications with working out which fields are used
in the output, we could only support this feature if the old style template
syntax is used. If Remap is used, the user won't have the option to remove these
fields from the message in the Loki sink.

## Doc-level Proposal

- Optional. Only do this if your change is public facing.
- Demonstrate how your change will look in the form of Vector's public docs.

A template string has two forms. To use our remap syntax you need to surround
the script with three brackets `{{{..}}}`. Any text within here is treated as a
Remap script. The result of the final statement needs to be a string that will
be used as the value of the given field.

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

The benefits are:

 - One familiar syntax and function reference for Vector.
 - Access to all of remap's functions for templating.
 - Less code to manage.


## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

[Handlebars JS](https://handlebarsjs.com/guide/#html-escaping) treats double
brackets (`{{..}}`) as one form of templating for most of it's language features,
but if you want special treatment (not escaping html) you can surround your
template fields with three brackets (`{{{..}}}`).


## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

There will be an additional maintenance burden. Should the need to track fields
used in the script be implemented that is a fairly significant complication to
the script execution process that will need to always be kept in mind when 
future additions to the language are being implemented.


## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

We do already have the existing template syntax. 

The advantage of using Remap for these fields are that it allows more 
flexibility in defining how the event is used. However, given that remap can be 
used as a transform, should the user really need this, they could put a Remap 
transform in the process to process these fields so they can be easily used in 
the template for the next phase.



### A global flag

We could specify a global flag within the config that indicates which language
the template fields will use. This could be either `template` or `trl`.

Initially, to maintain backward compatibility this could default to `template`.
Over time as more people get used to `trl` this can be changed to `trl`.

Note if `trl` is chosen, it would still need to detect when users are just
providing a hardcoded string that doesn't need templating. There would need to
be a way to differentiate between strings and erroneous attempts at Remap.

### A local flag

We could allow each field to specify which language that field uses. For example,

```
  labels.key = "value" # example
  labels.key = "{{ event_field }}"
  labels.key.lang = "template"
```

Note, it could be possible to allow a global flag to be set, but override that
setting at a field level.

### Returning fields at load time

To fix the issue with returning fields so that the Loki sink removes them, whilst
Remap is parsing the script, it could keep track of all fields that are being
used in the script.

This script, `if .foo { .bar } else { .baz }`, would result in all three
fields being returned - `.foo`, `.bar` and `.baz`, and subsequently removed
from the message sent to Loki.

### Returning fields at run time

Remap could keep track of all the fields that are read from as it runs and
return these fields. So the script, `if .foo { .bar } else { .baz }`,
would result in `.foo` and either `.bar` or `.baz` being returned.

If necessary, Remap could distinguish between fields that are used in the
condition and those used it the result, so only `.bar` or `.baz` could be
returned.

There are likely to be a number of edge cases that would need to be thought
through if we took this approach.



## Outstanding Questions

- List any remaining questions that you have.
- These must be resolved before the RFC can be merged.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.
