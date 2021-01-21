//! TODO: test each of the features separately, default features and no features at all.

#[test]
fn compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
