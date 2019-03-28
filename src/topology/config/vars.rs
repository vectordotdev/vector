// Much of this code is adapted from the [regex crate][0], so its license is included below.
// [0]: https://github.com/rust-lang/regex/blob/9b951a6bf6fc814eb43cbf4eb767ef693b748e8b/src/expand.rs
//
// Copyright (c) 2014 The Rust Project Developers
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use memchr::memchr;
use std::collections::HashMap;

pub fn interpolate(mut input: &str, vars: &HashMap<String, String>) -> String {
    let mut output = String::new();

    // while we haven't worked through the whole input
    while !input.is_empty() {
        // find the next position that could be a variable
        match memchr(b'$', input.as_bytes()) {
            None => break,
            Some(i) => {
                // add the input up to this point to the output
                output.push_str(&input[..i]);
                // move our "cursor" up to the var location
                input = &input[i..];
            }
        }
        // if the second char is also '$', it's an escaped literal
        if input.as_bytes().get(1).map_or(false, |&b| b == b'$') {
            output.push_str("$");
            input = &input[2..];
            continue;
        }
        debug_assert!(!input.is_empty());
        // find the variable referenced by our input
        let (var, end) = match find_var(input) {
            Some(hit) => hit,
            None => {
                // if there wasn't one, pass the literal input through
                output.push_str("$");
                input = &input[1..];
                continue;
            }
        };
        // scroll us past the variable name
        input = &input[end..];
        // if we have a value for that variable name, insert it, otherwise NULL
        if let Some(s) = vars.get(var) {
            output.push_str(s);
        }
    }
    // pass through any remaining input
    output.push_str(input);

    output
}

// parse out a variable name from the beginning of the string, returning the name and the ending
// position of the variable in the input (can differ from the length of the name due to braces)
fn find_var(input: &str) -> Option<(&str, usize)> {
    let mut i = 0;
    let input = input.as_bytes();
    if input.len() <= 1 || input[0] != b'$' {
        return None;
    }
    let mut brace = false;
    i += 1;
    if input[i] == b'{' {
        brace = true;
        i += 1;
    }
    let mut end = i;
    while input.get(end).map_or(false, is_valid_var_letter) {
        end += 1;
    }
    if end == i {
        return None;
    }
    // We just verified that the range 0..end is valid ASCII, so it must
    // therefore be valid UTF-8. If we really cared, we could avoid this UTF-8
    // check with either unsafe or by parsing the number straight from &[u8].
    let var = std::str::from_utf8(&input[i..end]).expect("valid UTF-8 variable name");
    if brace {
        if !input.get(end).map_or(false, |&b| b == b'}') {
            return None;
        }
        end += 1;
    }
    Some((var, end))
}

fn is_valid_var_letter(b: &u8) -> bool {
    match *b {
        b'0'...b'9' | b'a'...b'z' | b'A'...b'Z' | b'_' => true,
        _ => false,
    }
}

#[cfg(test)]
mod test {
    use super::interpolate;
    #[test]
    fn interpolation() {
        let vars = vec![
            ("FOO".into(), "dogs".into()),
            ("FOOBAR".into(), "cats".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!("dogs", interpolate("$FOO", &vars));
        assert_eq!("dogs", interpolate("${FOO}", &vars));
        assert_eq!("cats", interpolate("${FOOBAR}", &vars));
        assert_eq!("xcatsy", interpolate("x${FOOBAR}y", &vars));
        assert_eq!("x", interpolate("x$FOOBARy", &vars));
        assert_eq!("$ x", interpolate("$ x", &vars));
        assert_eq!("$FOO", interpolate("$$FOO", &vars));
        assert_eq!("", interpolate("$NOT_FOO", &vars));
        assert_eq!("-FOO", interpolate("$NOT-FOO", &vars));
        assert_eq!("${FOO x", interpolate("${FOO x", &vars));
        assert_eq!("${}", interpolate("${}", &vars));
    }
}
