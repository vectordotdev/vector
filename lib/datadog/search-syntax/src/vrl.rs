/// Build a VRL expression from a `&QueryNode`. Will recurse through each leaf element
/// as required.
pub fn build(node: &QueryNode) -> ast::Expr {
    recurse(parse_node(&node).into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, parse};
    use vrl_parser::ast;

    // Lhs = Datadog syntax. Rhs = VRL equivalent.
    static TESTS: &[(&str, &str)] = &[
        // Match everything (empty).
        ("", "true"),
        // Match everything.
        ("*:*", "true"),
        // Match everything (negate).
        ("NOT(*:*)", "false"),
        // Match nothing.
        ("-*:*", "false"),
        // Tag exists.
        ("_exists_:a", "exists(.__datadog_tags.a)"),
        // Tag exists (negate).
        ("NOT _exists_:a", "!exists(.__datadog_tags.a)"),
        // Tag exists (negate w/-).
        ("-_exists_:a", "!exists(.__datadog_tags.a)"),
        // Facet exists.
        ("_exists_:@b", "exists(.custom.b)"),
        // Facet exists (negate).
        ("NOT _exists_:@b", "!exists(.custom.b)"),
        // Facet exists (negate w/-).
        ("-_exists_:@b", "!exists(.custom.b)"),
        // Tag doesn't exist.
        ("_missing_:a", "!exists(.__datadog_tags.a)"),
        // Tag doesn't exist (negate).
        ("NOT _missing_:a", "!!exists(.__datadog_tags.a)"),
        // Tag doesn't exist (negate w/-).
        ("-_missing_:a", "!!exists(.__datadog_tags.a)"),
        // Facet doesn't exist.
        ("_missing_:@b", "!exists(.custom.b)"),
        // Facet doesn't exist (negate).
        ("NOT _missing_:@b", "!!exists(.custom.b)"),
        // Facet doesn't exist (negate w/-).
        ("-_missing_:@b", "!!exists(.custom.b)"),
        // Keyword.
        ("bla", r#"((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Keyword (negate).
        ("NOT bla", r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Keyword (negate w/-).
        ("-bla", r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Quoted keyword.
        (r#""bla""#, r#"((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Quoted keyword (negate).
        (r#"NOT "bla""#, r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Quoted keyword (negate w/-).
        (r#"-"bla""#, r#"!((match(.message, r'\bbla\b') ?? false) || ((match(.custom.error.message, r'\bbla\b') ?? false) || ((match(.custom.error.stack, r'\bbla\b') ?? false) || ((match(.custom.title, r'\bbla\b') ?? false) || (match(._default_, r'\bbla\b') ?? false)))))"#),
        // Tag match.
        ("a:bla", r#".__datadog_tags.a == "bla""#),
        // Reserved tag match.
        ("host:foo", r#".host == "foo""#),
        // Tag match (negate).
        ("NOT a:bla", r#"!(.__datadog_tags.a == "bla")"#),
        // Reserved tag match (negate).
        ("NOT host:foo", r#"!(.host == "foo")"#),
        // Tag match (negate w/-).
        ("-a:bla", r#"!(.__datadog_tags.a == "bla")"#),
        // Reserved tag match (negate w/-).
        ("-trace_id:foo", r#"!(.trace_id == "foo")"#),
        // Quoted tag match.
        (r#"a:"bla""#, r#".__datadog_tags.a == "bla""#),
        // Quoted tag match (negate).
        (r#"NOT a:"bla""#, r#"!(.__datadog_tags.a == "bla")"#),
        // Quoted tag match (negate w/-).
        (r#"-a:"bla""#, r#"!(.__datadog_tags.a == "bla")"#),
        // Facet match.
        ("@a:bla", r#".custom.a == "bla""#),
        // Facet match (negate).
        ("NOT @a:bla", r#"!(.custom.a == "bla")"#),
        // Facet match (negate w/-).
        ("-@a:bla", r#"!(.custom.a == "bla")"#),
        // Quoted facet match.
        (r#"@a:"bla""#, r#".custom.a == "bla""#),
        // Quoted facet match (negate).
        (r#"NOT @a:"bla""#, r#"!(.custom.a == "bla")"#),
        // Quoted facet match (negate w/-).
        (r#"-@a:"bla""#, r#"!(.custom.a == "bla")"#),
        // Wildcard prefix.
        ("*bla", r#"((match(.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.stack, r'\b.*bla\b') ?? false) || ((match(.custom.title, r'\b.*bla\b') ?? false) || (match(._default_, r'\b.*bla\b') ?? false)))))"#),
        // Wildcard prefix (negate).
        ("NOT *bla", r#"!((match(.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.stack, r'\b.*bla\b') ?? false) || ((match(.custom.title, r'\b.*bla\b') ?? false) || (match(._default_, r'\b.*bla\b') ?? false)))))"#),
        // Wildcard prefix (negate w/-).
        ("-*bla", r#"!((match(.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.message, r'\b.*bla\b') ?? false) || ((match(.custom.error.stack, r'\b.*bla\b') ?? false) || ((match(.custom.title, r'\b.*bla\b') ?? false) || (match(._default_, r'\b.*bla\b') ?? false)))))"#),
        // Wildcard suffix.
        ("bla*", r#"((match(.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.stack, r'\bbla.*\b') ?? false) || ((match(.custom.title, r'\bbla.*\b') ?? false) || (match(._default_, r'\bbla.*\b') ?? false)))))"#),
        // Wildcard suffix (negate).
        ("NOT bla*", r#"!((match(.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.stack, r'\bbla.*\b') ?? false) || ((match(.custom.title, r'\bbla.*\b') ?? false) || (match(._default_, r'\bbla.*\b') ?? false)))))"#),
        // Wildcard suffix (negate w/-).
        ("-bla*", r#"!((match(.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.message, r'\bbla.*\b') ?? false) || ((match(.custom.error.stack, r'\bbla.*\b') ?? false) || ((match(.custom.title, r'\bbla.*\b') ?? false) || (match(._default_, r'\bbla.*\b') ?? false)))))"#),
        // Multiple wildcards.
        ("*b*la*", r#"((match(.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.stack, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.title, r'\b.*b.*la.*\b') ?? false) || (match(._default_, r'\b.*b.*la.*\b') ?? false)))))"#),
        // Multiple wildcards (negate).
        ("NOT *b*la*", r#"!((match(.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.stack, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.title, r'\b.*b.*la.*\b') ?? false) || (match(._default_, r'\b.*b.*la.*\b') ?? false)))))"#),
        // Multiple wildcards (negate w/-).
        ("-*b*la*", r#"!((match(.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.message, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.error.stack, r'\b.*b.*la.*\b') ?? false) || ((match(.custom.title, r'\b.*b.*la.*\b') ?? false) || (match(._default_, r'\b.*b.*la.*\b') ?? false)))))"#),
        // Wildcard prefix - tag.
        ("a:*bla", r#"(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard prefix - tag (negate).
        ("NOT a:*bla", r#"!(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard prefix - tag (negate w/-).
        ("-a:*bla", r#"!(ends_with(.__datadog_tags.a, "bla") ?? false)"#),
        // Wildcard suffix - tag.
        ("b:bla*", r#"(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Wildcard suffix - tag (negate).
        ("NOT b:bla*", r#"!(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Wildcard suffix - tag (negate w/-).
        ("-b:bla*", r#"!(starts_with(.__datadog_tags.b, "bla") ?? false)"#),
        // Multiple wildcards - tag.
        ("c:*b*la*", r#"(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - tag (negate).
        ("NOT c:*b*la*", r#"!(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - tag (negate w/-).
        ("-c:*b*la*", r#"!(match(.__datadog_tags.c, r'^.*b.*la.*$') ?? false)"#),
        // Wildcard prefix - facet.
        ("@a:*bla", r#"(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard prefix - facet (negate).
        ("NOT @a:*bla", r#"!(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard prefix - facet (negate w/-).
        ("-@a:*bla", r#"!(ends_with(.custom.a, "bla") ?? false)"#),
        // Wildcard suffix - facet.
        ("@b:bla*", r#"(starts_with(.custom.b, "bla") ?? false)"#),
        // Wildcard suffix - facet (negate).
        ("NOT @b:bla*", r#"!(starts_with(.custom.b, "bla") ?? false)"#),
        // Wildcard suffix - facet (negate w/-).
        ("-@b:bla*", r#"!(starts_with(.custom.b, "bla") ?? false)"#),
        // Multiple wildcards - facet.
        ("@c:*b*la*", r#"(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - facet (negate).
        ("NOT @c:*b*la*", r#"!(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Multiple wildcards - facet (negate w/-).
        ("-@c:*b*la*", r#"!(match(.custom.c, r'^.*b.*la.*$') ?? false)"#),
        // Special case for tags.
        ("tags:a", r#"(includes(.tags, "a") ?? false)"#),
        // Special case for tags (negate).
        ("NOT tags:a", r#"!(includes(.tags, "a") ?? false)"#),
        // Special case for tags (negate w/-).
        ("-tags:a", r#"!(includes(.tags, "a") ?? false)"#),
        // Range - numeric, inclusive.
        ("[1 TO 10]", r#"(((.message >= "1" && .message <= "10") ?? false) || (((.custom.error.message >= "1" && .custom.error.message <= "10") ?? false) || (((.custom.error.stack >= "1" && .custom.error.stack <= "10") ?? false) || (((.custom.title >= "1" && .custom.title <= "10") ?? false) || ((._default_ >= "1" && ._default_ <= "10") ?? false)))))"#),
        // Range - numeric, inclusive (negate).
        ("NOT [1 TO 10]", r#"!(((.message >= "1" && .message <= "10") ?? false) || (((.custom.error.message >= "1" && .custom.error.message <= "10") ?? false) || (((.custom.error.stack >= "1" && .custom.error.stack <= "10") ?? false) || (((.custom.title >= "1" && .custom.title <= "10") ?? false) || ((._default_ >= "1" && ._default_ <= "10") ?? false)))))"#),
        // Range - numeric, inclusive (negate w/-).
        ("-[1 TO 10]", r#"!(((.message >= "1" && .message <= "10") ?? false) || (((.custom.error.message >= "1" && .custom.error.message <= "10") ?? false) || (((.custom.error.stack >= "1" && .custom.error.stack <= "10") ?? false) || (((.custom.title >= "1" && .custom.title <= "10") ?? false) || ((._default_ >= "1" && ._default_ <= "10") ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (upper).
        ("[50 TO *]", r#"((.message >= "50" ?? false) || ((.custom.error.message >= "50" ?? false) || ((.custom.error.stack >= "50" ?? false) || ((.custom.title >= "50" ?? false) || (._default_ >= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (upper) (negate).
        ("NOT [50 TO *]", r#"!((.message >= "50" ?? false) || ((.custom.error.message >= "50" ?? false) || ((.custom.error.stack >= "50" ?? false) || ((.custom.title >= "50" ?? false) || (._default_ >= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (upper) (negate w/-).
        ("-[50 TO *]", r#"!((.message >= "50" ?? false) || ((.custom.error.message >= "50" ?? false) || ((.custom.error.stack >= "50" ?? false) || ((.custom.title >= "50" ?? false) || (._default_ >= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (lower).
        ("[* TO 50]", r#"((.message <= "50" ?? false) || ((.custom.error.message <= "50" ?? false) || ((.custom.error.stack <= "50" ?? false) || ((.custom.title <= "50" ?? false) || (._default_ <= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (lower) (negate).
        ("NOT [* TO 50]", r#"!((.message <= "50" ?? false) || ((.custom.error.message <= "50" ?? false) || ((.custom.error.stack <= "50" ?? false) || ((.custom.title <= "50" ?? false) || (._default_ <= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (lower) (negate w/-).
        ("-[* TO 50]", r#"!((.message <= "50" ?? false) || ((.custom.error.message <= "50" ?? false) || ((.custom.error.stack <= "50" ?? false) || ((.custom.title <= "50" ?? false) || (._default_ <= "50" ?? false)))))"#),
        // Range - numeric, inclusive, unbounded (both).
        ("[* TO *]", "(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate).
        ("NOT [* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, unbounded (both) (negate w/-).
        ("-[* TO *]", "!(exists(.message) || (exists(.custom.error.message) || (exists(.custom.error.stack) || (exists(.custom.title) || exists(._default_)))))"),
        // Range - numeric, inclusive, tag.
        ("a:[1 TO 10]", r#"((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, tag (negate).
        ("NOT a:[1 TO 10]", r#"!((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, tag (negate w/-).
        ("-a:[1 TO 10]", r#"!((.__datadog_tags.a >= "1" && .__datadog_tags.a <= "10") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag.
        ("a:[50 TO *]", r#"(.__datadog_tags.a >= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag (negate).
        ("NOT a:[50 TO *]", r#"!(.__datadog_tags.a >= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), tag (negate w/-).
        ("-a:[50 TO *]", r#"!(.__datadog_tags.a >= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag.
        ("a:[* TO 50]", r#"(.__datadog_tags.a <= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag (negate).
        ("NOT a:[* TO 50]", r#"!(.__datadog_tags.a <= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), tag (negate w/-).
        ("-a:[* TO 50]", r#"!(.__datadog_tags.a <= "50" ?? false)"#),
        // Range - numeric, inclusive, unbounded (both), tag.
        ("a:[* TO *]", "exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("NOT a:[* TO *]", "!exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, unbounded (both), tag (negate).
        ("-a:[* TO *]", "!exists(.__datadog_tags.a)"),
        // Range - numeric, inclusive, facet.
        ("@b:[1 TO 10]", r#"(((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 1) || .custom.b >= "1") && (((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 10) || .custom.b <= "10")) ?? false)"#),
        // Range - numeric, inclusive, facet (negate).
        ("NOT @b:[1 TO 10]", r#"!(((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 1) || .custom.b >= "1") && (((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 10) || .custom.b <= "10")) ?? false)"#),
        // Range - numeric, inclusive, facet (negate w/-).
        ("-@b:[1 TO 10]", r#"!(((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 1) || .custom.b >= "1") && (((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 10) || .custom.b <= "10")) ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), facet.
        ("@b:[50 TO *]", r#"((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 50) || .custom.b >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), facet (negate).
        ("NOT @b:[50 TO *]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 50) || .custom.b >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (upper), facet (negate w/-).
        ("-@b:[50 TO *]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b >= 50) || .custom.b >= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), facet.
        ("@b:[* TO 50]", r#"((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 50) || .custom.b <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), facet (negate).
        ("NOT @b:[* TO 50]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 50) || .custom.b <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (lower), facet (negate w/-).
        ("-@b:[* TO 50]", r#"!((((is_integer(.custom.b) || is_float(.custom.b)) && .custom.b <= 50) || .custom.b <= "50") ?? false)"#),
        // Range - numeric, inclusive, unbounded (both), facet.
        ("@b:[* TO *]", "exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate).
        ("NOT @b:[* TO *]", "!exists(.custom.b)"),
        // Range - numeric, inclusive, unbounded (both), facet (negate w/-).
        ("-@b:[* TO *]", "!exists(.custom.b)"),
        // Range - tag, exclusive
        ("f:{1 TO 10}", r#"((.__datadog_tags.f > "1" && .__datadog_tags.f < "10") ?? false)"#),
        // Range - facet, exclusive
        ("@f:{1 TO 10}", r#"(((((is_integer(.custom.f) || is_float(.custom.f)) && .custom.f > 1) || .custom.f > "1") && (((is_integer(.custom.f) || is_float(.custom.f)) && .custom.f < 10) || .custom.f < "10")) ?? false)"#),
        // Range - alpha, inclusive
        (r#"g:[a TO z]"#, r#"((.__datadog_tags.g >= "a" && .__datadog_tags.g <= "z") ?? false)"#),
        // Range - alpha, exclusive
        (r#"g:{a TO z}"#, r#"((.__datadog_tags.g > "a" && .__datadog_tags.g < "z") ?? false)"#),
        // Range - alpha, inclusive (quoted)
        (r#"g:["a" TO "z"]"#, r#"((.__datadog_tags.g >= "a" && .__datadog_tags.g <= "z") ?? false)"#),
        // Range - alpha, exclusive (quoted)
        (r#"g:{"a" TO "z"}"#, r#"((.__datadog_tags.g > "a" && .__datadog_tags.g < "z") ?? false)"#),
        // AND match, known tags.
        (
            "message:this AND @title:that",
            r#"((match(.message, r'\bthis\b') ?? false) && (match(.custom.title, r'\bthat\b') ?? false))"#
        ),
        // OR match, known tags.
        (
            "message:this OR @title:that",
            r#"((match(.message, r'\bthis\b') ?? false) || (match(.custom.title, r'\bthat\b') ?? false))"#
        ),
        // AND + OR match, nested, known tags.
        (
            "message:this AND (@title:that OR @title:the_other)",
            r#"((match(.message, r'\bthis\b') ?? false) && ((match(.custom.title, r'\bthat\b') ?? false) || (match(.custom.title, r'\bthe_other\b') ?? false)))"#
        ),
        // AND match, keyword.
        (
            "this AND that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && ((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // AND match, keyword (negate last).
        (
            "this AND NOT that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && !((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // AND match, keyword (negate last w/-).
        (
            "this AND -that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && !((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // OR match, keyword, explicit.
        (
            "this OR that",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) || ((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))))"#,
        ),
        // AND and OR match.
        (
            "this AND (that OR the_other)",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) && (((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))) || ((match(.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.stack, r'\bthe_other\b') ?? false) || ((match(.custom.title, r'\bthe_other\b') ?? false) || (match(._default_, r'\bthe_other\b') ?? false)))))))"#,
        ),
        // OR and AND match.
        (
            "this OR (that AND the_other)",
            r#"(((match(.message, r'\bthis\b') ?? false) || ((match(.custom.error.message, r'\bthis\b') ?? false) || ((match(.custom.error.stack, r'\bthis\b') ?? false) || ((match(.custom.title, r'\bthis\b') ?? false) || (match(._default_, r'\bthis\b') ?? false))))) || (((match(.message, r'\bthat\b') ?? false) || ((match(.custom.error.message, r'\bthat\b') ?? false) || ((match(.custom.error.stack, r'\bthat\b') ?? false) || ((match(.custom.title, r'\bthat\b') ?? false) || (match(._default_, r'\bthat\b') ?? false))))) && ((match(.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.message, r'\bthe_other\b') ?? false) || ((match(.custom.error.stack, r'\bthe_other\b') ?? false) || ((match(.custom.title, r'\bthe_other\b') ?? false) || (match(._default_, r'\bthe_other\b') ?? false)))))))"#,
        ),
        // A bit of everything.
        (
            "host:this OR ((@b:test* AND c:that) AND d:the_other @e:[1 TO 5])",
            r#"(.host == "this" || (((starts_with(.custom.b, "test") ?? false) && .__datadog_tags.c == "that") && (.__datadog_tags.d == "the_other" && (((((is_integer(.custom.e) || is_float(.custom.e)) && .custom.e >= 1) || .custom.e >= "1") && (((is_integer(.custom.e) || is_float(.custom.e)) && .custom.e <= 5) || .custom.e <= "5")) ?? false))))"#,
        ),
    ];

    #[test]
    /// Compile each Datadog search query -> VRL, and do the same with the equivalent direct
    /// VRL syntax, and then compare the results. Each expression should match identically to
    /// the debugging output.
    fn to_vrl() {
        for (dd, vrl) in TESTS.iter() {
            let node =
                parse(dd).unwrap_or_else(|_| panic!("invalid Datadog search syntax: {}", dd));

            let root = ast::RootExpr::Expr(make_node(build(&node)));

            let program = vrl_parser::parse(vrl).unwrap_or_else(|_| panic!("invalid VRL: {}", vrl));

            assert_eq!(
                format!("{:?}", vec![make_node(root)]),
                format!("{:?}", program.0),
                "Failed: DD= {}, VRL= {}",
                dd,
                vrl
            );
        }
    }

    #[test]
    /// Test that the program compiles, and has the right number of expressions (which should
    /// be initial Datadog tags parsing, and a subsequent query against tags and other fields.)
    fn compiles() {
        for (dd, _) in TESTS.iter() {
            let node = parse(dd).unwrap();

            let program = compile(build(&node))
                .unwrap_or_else(|e| panic!("failed to compile: '{}'. Errors: {:?}", dd, e));

            assert!(program.into_iter().len() == 2);
        }
    }
}
