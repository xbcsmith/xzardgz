// ruleid: rust-shell-command-injection
let _ = Command::new("sh");
// ruleid: rust-shell-command-injection
let _ = Command::new("bash");
// ok: direct command without shell is safe
let _ = Command::new("/usr/bin/myapp");
