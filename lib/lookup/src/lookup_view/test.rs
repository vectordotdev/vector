use std::{fs, io::Read, path::Path};

use tracing::trace;

use crate::*;

const SUFFICIENTLY_COMPLEX: &str = r#"regular."quoted"."quoted but spaces"."quoted.but.periods".lookup[0].00numericstart.nested_lookup[0][0]"#;

lazy_static::lazy_static! {
    static ref SUFFICIENTLY_DECOMPOSED: [Segment<'static>; 10] = [
        Segment::from(r#"regular"#),
        Segment::from(r#""quoted""#),
        Segment::from(r#""quoted but spaces""#),
        Segment::from(r#""quoted.but.periods""#),
        Segment::from(r#"lookup"#),
        Segment::from(0),
        Segment::from("00numericstart"),
        Segment::from(r#"nested_lookup"#),
        Segment::from(0),
        Segment::from(0),
    ];
}

#[test]
fn field_is_quoted() {
    let field: Field = "zork2".into();
    assert_eq!(
        Field {
            name: "zork2",
            requires_quoting: false,
        },
        field
    );

    let field: Field = "zork2-zoog".into();
    assert_eq!(
        Field {
            name: "zork2-zoog",
            requires_quoting: true,
        },
        field
    );
}

#[test]
fn zero_len_not_allowed() {
    let input = "";
    let maybe_lookup = Lookup::from_str(input);
    assert!(maybe_lookup.is_err());
}

#[test]
fn unquoted_numeric_not_allowed() {
    let input = "23";
    let maybe_lookup = Lookup::from_str(input);
    assert!(maybe_lookup.is_err());
}

#[test]
fn we_dont_parse_plain_strings_in_from() {
    let input = "some_key.still_the_same_key.this.is.going.in.via.from.and.should.not.get.parsed";
    let output =
        r#""some_key.still_the_same_key.this.is.going.in.via.from.and.should.not.get.parsed""#;
    let lookup = Lookup::from(input);
    assert_eq!(lookup[0], Segment::from(input));
    assert_eq!(lookup.to_string(), output);
}

#[test]
fn simple() {
    let input = "some_key";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("some_key"));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn unquoted() {
    let input = "start.after";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("start"));
    assert_eq!(lookup[1], Segment::from("after"));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn quoted() {
    let input = "\"start\".\"after\"";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("\"start\""));
    assert_eq!(lookup[1], Segment::from("\"after\""));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn quoted_special_characters() {
    let input = "\"start@noog\".\"after nork\"";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("\"start@noog\""));
    assert_eq!(lookup[1], Segment::from("\"after nork\""));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn push() {
    let input = "some_key";
    let mut lookup = Lookup::from_str(input).unwrap();
    lookup.push_back(Segment::from(input));
    assert_eq!(lookup[0], Segment::from("some_key"));
    assert_eq!(lookup[1], Segment::from("some_key"));
}

#[test]
fn pop() {
    let input = "some_key";
    let mut lookup = Lookup::from_str(input).unwrap();
    let out = lookup.pop_back();
    assert_eq!(out, Some(Segment::from("some_key")));
}

#[test]
fn array() {
    let input = "foo[0]";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("foo"));
    assert_eq!(lookup[1], Segment::from(0));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn fields() {
    let input = "florp.flop";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("florp"));
    assert_eq!(lookup[1], Segment::from("flop"));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn fields_with_quotes() {
    let input = "florp.\"flop fleep\"";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("florp"));
    assert_eq!(lookup[1], Segment::from(r#""flop fleep""#));
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn coalesced() {
    let input = "plain.(option_one | option_two)";
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup[0], Segment::from("plain"));
    assert_eq!(
        lookup[1],
        Segment::from(vec![Field::from("option_one"), Field::from("option_two"),])
    );
}

#[test]
fn coalesced_nesting() {
    let input = "plain.(option_one.inner | option_two.other_inner)";
    assert!(Lookup::from_str(input).is_err());
}

#[test]
fn to_string() {
    let input = SUFFICIENTLY_COMPLEX;
    let lookup = Lookup::from_str(input).unwrap();
    assert_eq!(lookup.to_string(), input);
}

#[test]
fn impl_index_usize() {
    let lookup = Lookup::from_str(SUFFICIENTLY_COMPLEX).unwrap();

    for i in 0..SUFFICIENTLY_DECOMPOSED.len() {
        assert_eq!(lookup[i], SUFFICIENTLY_DECOMPOSED[i])
    }
}

#[test]
fn impl_index_mut_index_mut() {
    let mut lookup = Lookup::from_str(SUFFICIENTLY_COMPLEX).unwrap();

    for i in 0..SUFFICIENTLY_DECOMPOSED.len() {
        let x = &mut lookup[i]; // Make sure we force a mutable borrow!
        assert_eq!(*x, SUFFICIENTLY_DECOMPOSED[i])
    }
}

#[test]
fn iter() {
    let lookup = Lookup::from_str(SUFFICIENTLY_COMPLEX).unwrap();

    let mut iter = lookup.iter();
    for (index, expected) in SUFFICIENTLY_DECOMPOSED.iter().enumerate() {
        let parsed = iter
            .next()
            .unwrap_or_else(|| panic!("Expected at index {}: {:?}, got None.", index, expected));
        assert_eq!(expected, parsed, "Failed at {}", index);
    }
}

#[test]
fn into_iter() {
    let lookup = Lookup::from_str(SUFFICIENTLY_COMPLEX).unwrap();
    let mut iter = lookup.into_iter();
    for (index, expected) in SUFFICIENTLY_DECOMPOSED.iter().cloned().enumerate() {
        let parsed = iter
            .next()
            .unwrap_or_else(|| panic!("Expected at index {}: {:?}, got None.", index, expected));
        assert_eq!(expected, parsed, "Failed at {}", index);
    }
}

fn parse_artifact(path: impl AsRef<Path>) -> std::io::Result<String> {
    let mut test_file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(e),
    };

    let mut buf = Vec::new();
    test_file.read_to_end(&mut buf)?;
    let string = String::from_utf8(buf).unwrap();
    // remove trailing newline introduced by editors
    Ok(string.trim_end().to_owned())
}

// This test iterates over the `tests/data/fixtures/lookup` folder and ensures the lookup parsed,
// then turned into a string again is the same.
#[test]
fn lookup_to_string_and_serialize() {
    const FIXTURE_ROOT: &str = "tests/fixtures/lookup";

    std::fs::read_dir(FIXTURE_ROOT)
        .unwrap()
        .for_each(|fixture_file| match fixture_file {
            Ok(fixture_file) => {
                let path = fixture_file.path();
                let buf = parse_artifact(&path).unwrap();

                let buf_serialized =
                    serde_json::to_string(&serde_json::to_value(&buf).unwrap()).unwrap();
                let lookup = Lookup::from_str(&buf).unwrap();
                assert_eq!(lookup.to_string(), buf);

                // **WARNING:**: You **can not** deserialize lookups (that is, views, the buffers
                // are fine) out of str slices with escapes. It's invalid. You **must** use lookupbufs.
                if lookup.to_string().contains('\"') {
                    trace!(
                        "Need to filter this test, it contains escape chars and cannot be \
                            deserialized into a lookup view. Use a LookupBuf."
                    );
                    return;
                }

                // Ensure serialization doesn't clobber.
                let serialized = serde_json::to_string(&lookup.to_string()).unwrap();
                assert_eq!(serialized, buf_serialized);
                // Ensure deserializing doesn't clobber.
                let deserialized: Lookup<'_> = serde_json::from_str(&serialized).unwrap();
                assert_eq!(lookup, deserialized);
            }
            _ => panic!("This test should never read Err'ing test fixtures."),
        });
}
