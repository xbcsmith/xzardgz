// Positive fixture for statement sequence ellipsis test.
// A function (without visibility modifier) containing multiple statements.

fn process() {
    let x = 1;
    let y = x + 2;
    let _result = y * 3;
}
