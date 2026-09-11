// Positive fixture for ellipsis (...) argument list rewriting.
// Pattern foo(...) should match foo called with any number of arguments.

pub fn foo(a: i32, b: i32, c: i32) -> i32 {
    a + b + c
}

pub fn caller() {
    let _x = foo(1, 2, 3);
    let _y = foo(10, 20, 30);
}
