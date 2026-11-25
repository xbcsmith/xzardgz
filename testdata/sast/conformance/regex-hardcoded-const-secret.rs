// ruleid: regex-hardcoded-const-secret
const API_KEY: &str = "abc123defghijklmno";
// ruleid: regex-hardcoded-const-secret
const SECRET_TOKEN: &str = "tok-xyz789";
// ok: environment variable is safe
const APP_VERSION: &str = "1.0.0";
