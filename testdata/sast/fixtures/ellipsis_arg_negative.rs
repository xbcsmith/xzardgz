// Negative fixture for ellipsis argument list test.
// Function bar is called, which should NOT match pattern: foo(...)

pub fn bar(a: i32) -> i32 {
    a * 2
}

pub fn caller() {
    let _y = bar(42);
}
