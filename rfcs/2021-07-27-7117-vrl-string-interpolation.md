# RFC 7117 - 2021-07-27 - VRL string interpolation

VRL needs a better way to format strings. Currently the only way to do this is
to concatenate strings, which can get unwieldy.

## Scope

This RFC discusses creating a new string type within VRL that can use template
literals, also known as string interpolation, to format strings.

## Pain

Currently the way to create strings is through either string concatenation or
the `join` function.

Syntactically this is unwieldy. It requires extra key presses and the code
created doesn't necessarily give an instant idea of what the resulting string
will look like. Thus the true intent behind the code is obfuscated, which can
result in bugs.

## User Experience

The initial version of string interpolation will be the simplest possible, allowing
for further expansion in the future should it be deemed useful.

We will allow interpolating only string variables in strings delimited by `"..."`.

Syntax would be as follows:

```coffee
"foo {{bar}}"
```

You can "escape" `{` and `}` to avoid interpolation using `\`:

```coffee
"foo \{{bar\}}"
```

Alternatively raw strings can be used which would not template:

```coffee
s'foo {bar}'
```

As mentioned, only variables are supported, any other expression evaluation has
to be done before interpolating:

```coffee
foobar = upcase("foo bar")
"{{ foobar }} BAZ"
Also, the variable has to resolve to an exact string type, and nothing else:
```

### not allowed

```coffee
number = 1
"{{ number }}"
```

### allowed

```coffee
number = to_string(1)
"{{ number }}"
```

## Implementation

This new string type can be considered as syntactic sugar for string
concatenation.

The VRL parser will take a template literal string such as:

```coffee
"The message is {{ message }} and we {{ feeling }} it"
```

and create an AST identical to the AST for the following expression:

```coffee
"The message is " +
message +
" and we " +
feeling +
" it"
```

### Fallibility

Due to the way the format strings are implemented as expanding to an expression
they are automatically fallible if the variable used is not a string.

```coffee
> thing = 3
> "The number is {{ thing }}" ?? "invalid string"

"invalid string"
```

This has little use at present, but with future work where expanded types are
allowed this could provide value.

## Rationale

String interpolation or string formats are prevalent in modern programming
languages. Users have an expectation that this feature will be available.

String formatting is a common task within VRL. Currently the process involves
string concatenation. This works, but the code required to do this does not
create an immediately apparent representation of what the string may look like.

There is little impact of not doing this beyond requiring users to use a less
elegant form for string creation.

## Prior Art

- Template strings are used within certain fields within Vector.
- Many programming languages offer string interpolation.
  - [Python](https://peps.python.org/pep-0498/)
  - [JavaScript](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Template_literals)
  - [Ruby](http://ruby-for-beginners.rubymonstas.org/bonus/string_interpolation.html)

## Drawbacks

This does add additional code complexity and an maintenance burden to VRL.

## Alternatives

### Format Strings

One alternative is to create a `sprintf` function. `sprintf` takes one parameter
that represents the format string. Within the format string if a format tag is
found, the function will take the next parameter passed and will format that
parameter according to the tag and will embed the resulting text in that
position.

For example:

```coffee
sprintf("The message is %s created at %t", .message, .timestamp)
```

will return

```coffee
The message is the message created at Tue, 27 Jul 2021 10:10:01 +0000
```

The advantages of this method is that it does not require any changes to the
VRL compiler, all changes are isolated to a single function. Also it provides
a way to influence the formatting of the parameters.

Downsides are that the format strings are a hidden DSL themselves and there is
a cognitive overhead involved in maintaining the position of the format tags
within the string and the parameters passed to the function.

### Output error text

Rather than forcing the user to handle errors, if an error occurs the error text is output.
For example:

```coffee
"This is some json {{ parse_json(.thing) }'
# This is some json function call error for "parse_json" at (0:18): unable to parse json: expected ident at line 1 column 2
```

### Use comprehensive f-strings

We will be loosely basing our format strings on Pythons [f-strings](https://peps.python.org/pep-0498/).

`f-strings` allow for a combination of embedded expressions and include the ability
to specify formatting options for the outputs. Plus, their use in a widespread language
should mean a lot of users will already be familiar with the functionality.

To format a string there will be a string type denoted with the prefix `f'`.

Within that string, it is possible to embed VRL expressions by surrounding them
with `{..}`. VRL will evaluate the expressions and will call `as_string` on
that expression to return the text representation.

```coffee
f'The message is { .message } created at { .timestamp }'
```

If you wish to actually insert a `{` into a string, a double '{{' will be needed.

```coffee
f'Here is a curly brace -> {{'
```

Since this is a new string type there are no backward compatibility issues.

#### Errors

We do not want an f string to be fallible as this would cumbersome to the experience of using VRL.

Each template segment must be infallible in order for the string to compile. Errors must be
handled to provide alternative text if needed:

```coffee
"This is some json {{ parse_json(.thing) ?? "oops" }}'
# This is some json oops
```

Another source of error would be if the format string is specified for a different
type - for example using date format strings when the type is an integer.

If format strings are provided, we need to lean on VRLs type system to ensure that the format
strings are valid for the given type. The user must ensure the types are coerced if
necessary.

For example this will not compile:

```coffee
thing = 2
f'The date is {thing: %v %R}.'
```

If needed the user will be expected to coerce the type:

```coffee
f'The date is {timestamp!(thing): %v %R}.'
```

This is a comprehensive solution, and would allow overly complex strings such as:

```coffee
f'This could { if contains(.thing, "zonk") ?? contains(.thunk, "zork") ?? false {
  foreach(.zeek) -> {
    f'My god what { object!(parse_json!(.zork)).zoog } is going on'
  }
  else {
...
} be a bit complex'
```

This is not in the spirit of VRL as it would lead to complex unmaintainable VRL code.

## Outstanding Questions


## Future work

### Expand to enable format strings

The current proposal only works with string variables. Future work can allow other types to be used
together with format strings

```coffee
"The message is message {{ number: %d }} created at {{ timestamp: %v %R }}"
```

### Allow templates in dynamic paths

Namely, we can allow the following dynamic path operations:

```coffee
.foo."{{ bar }}"[index]
```

Where bar has to resolve to a string, and index has to resolve to an integer. Both variables are known
to exist and be of the correct type, so we can keep path access infallible this way, and since we
already support the ."foo bar" syntax for path segments with special characters, there also won't
be a breaking syntax change, or any additional syntax that needs to be added to support this.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR that updates the lexer, parser and rewrites the AST for template strings to perform
      string concatenation instead.
