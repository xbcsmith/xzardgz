// ruleid: regex-hardcoded-db-url
let url = "postgres://user:password@localhost/mydb";
// ruleid: regex-hardcoded-db-url
let conn = "mysql://admin:secret@db.example.com/app";
// ok: reading connection string from environment is safe
let url = std::env::var("DATABASE_URL").unwrap_or_default();
