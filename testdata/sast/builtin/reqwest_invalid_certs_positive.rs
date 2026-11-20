use reqwest::ClientBuilder;

fn build_insecure_client() -> reqwest::Client {
    ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
}
