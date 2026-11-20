fn get_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_default()
}
