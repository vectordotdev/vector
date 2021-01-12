#[macro_use]
extern crate derive_is_enum_variant;

/// A kind of pet.
#[derive(is_enum_variant)]
pub enum Pet {
    /// A dog.
    Doggo,
    /// A cat.
    Kitteh,
    /// A flying squirrel.
    FlyingSquirrel,
}

#[test]
fn basic_enum_predicates() {
    let doggo = Pet::Doggo;
    assert!(doggo.is_doggo());
    assert!(!doggo.is_kitteh());
    assert!(!doggo.is_flying_squirrel());

    let kitteh = Pet::Kitteh;
    assert!(!kitteh.is_doggo());
    assert!(kitteh.is_kitteh());
    assert!(!kitteh.is_flying_squirrel());

    let squirrel = Pet::FlyingSquirrel;
    assert!(!squirrel.is_doggo());
    assert!(!squirrel.is_kitteh());
    assert!(squirrel.is_flying_squirrel());
}

/// Different kinds of enum variants.
#[derive(is_enum_variant)]
pub enum VariantKinds {
    Struct { x: usize, y: usize },
    Tuple(usize, usize),
    Unit,
}

#[test]
fn variant_kinds() {
    assert!(VariantKinds::Struct { x: 1, y: 2 }.is_struct());
    assert!(VariantKinds::Tuple(1, 2).is_tuple());
    assert!(VariantKinds::Unit.is_unit());
}

/// Various funky case names.
#[allow(non_camel_case_types)]
#[derive(is_enum_variant)]
pub enum Funky {
    /// doc
    CAPS,
    /// doc
    SHOUTING_SNAKE,
    /// doc
    snake_case,
    /// doc
    littleCamel,
    /// doc
    WithACRONYM,
}

#[test]
fn funky_variant_names() {
    assert!(Funky::CAPS.is_caps());
    assert!(Funky::SHOUTING_SNAKE.is_shouting_snake());
    assert!(Funky::snake_case.is_snake_case());
    assert!(Funky::littleCamel.is_little_camel());
    assert!(Funky::WithACRONYM.is_with_acronym());
}

/// Test providing custom predicate names.
#[derive(is_enum_variant)]
pub enum CustomNames {
    #[is_enum_variant(name = "i_dont_know_why_you_say")]
    Goodbye,
    #[is_enum_variant(name = "i_say")]
    Hello,
}

#[test]
fn custom_predicate_names() {
    assert!(CustomNames::Goodbye.i_dont_know_why_you_say());
    assert!(CustomNames::Hello.i_say());
}

/// This doesn't get a predicate for every variant
#[derive(is_enum_variant)]
pub enum Skip {
    #[is_enum_variant(skip)]
    NoPredicate,
    YesPredicate,
}

#[test]
fn skip_variants() {
    assert!(Skip::YesPredicate.is_yes_predicate());
}

/// Because there is only one variant, the generated match's `_` pattern is
/// unreachable. This better not create a compilation error due to our
/// `deny(unreachable_patterns)`.
#[derive(is_enum_variant)]
pub enum GeneratedCodeHasNoWarnings {
    OnlyOneVariant,
}
