// ruleid: regex-hardcoded-secret-assignment
let password = "hunter2password";
// ruleid: regex-hardcoded-secret-assignment
let api_key = "sk-abc123def456";
// ok: reading from environment is safe
let password = std::env::var("PASSWORD").unwrap_or_default();
