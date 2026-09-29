//! Shared loader for the JSON test cases from the Java test suite.

// Each test crate uses a subset of these helpers.
#![allow(dead_code)]

mod json;

/// One test case from the JSON files, read from the keys `testID`, `t1`,
/// `t2` and `d`.
#[derive(Debug)]
pub struct TestCase {
    /// Identifier to find a failed test case in the JSON file.
    pub test_id: i32,
    /// Source tree in bracket notation.
    pub t1: String,
    /// Destination tree in bracket notation.
    pub t2: String,
    /// Correct distance between source and destination trees.
    pub d: i32,
}

impl std::fmt::Display for TestCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "testID:{},t1:{},t2:{},d:{}",
            self.test_id, self.t1, self.t2, self.d
        )
    }
}

pub fn load(file: &str) -> Vec<TestCase> {
    let path = format!("{}/tests/resources/{}", env!("CARGO_MANIFEST_DIR"), file);
    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    json::parse_cases(&data).unwrap_or_else(|e| panic!("parsing {path}: {e}"))
}

/// Runs `check` on every case (the Java suite is parameterized) and fails
/// with the list of every failing case.
pub fn for_each_case(file: &str, check: impl Fn(&TestCase) -> Result<(), String>) {
    let cases = load(file);
    assert!(!cases.is_empty());
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|tc| check(tc).err().map(|e| format!("[{tc}] {e}")))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

pub fn expect_eq<T: PartialEq + std::fmt::Debug>(
    expected: T,
    actual: T,
    what: &str,
) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!("{what}: expected {expected:?}, got {actual:?}"))
    }
}
