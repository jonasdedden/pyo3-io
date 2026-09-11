//! Every capability the type does not carry is a compile error, not a runtime one.

#[test]
fn capabilities_are_enforced_at_compile_time() {
    let cases = trybuild::TestCases::new();
    let mut paths: Vec<_> = std::fs::read_dir("tests/ui")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    paths.sort();
    for path in paths {
        let name = path.file_stem().unwrap().to_str().unwrap();
        if !cfg!(unix) && (name.starts_with("as_fd_") || name.starts_with("clone_fd_")) {
            continue;
        }
        cases.compile_fail(path);
    }
}
