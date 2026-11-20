use std::process::Command;

fn run_script(script: &str) {
    Command::new("sh").arg("-c").arg(script).output().unwrap();
}
