fn get_key() -> String {
    std::env::var("API_KEY").unwrap_or_default()
}
