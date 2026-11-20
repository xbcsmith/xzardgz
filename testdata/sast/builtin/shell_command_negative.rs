use std::process::Command;

fn run_grep(pattern: &str, file: &str) {
    Command::new("grep")
        .arg(pattern)
        .arg(file)
        .output()
        .unwrap();
}
