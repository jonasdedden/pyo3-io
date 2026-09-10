//! Every capability the type does not carry is a compile error, not a runtime one.

#[test]
fn capabilities_are_enforced_at_compile_time() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
