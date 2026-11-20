use reqwest::ClientBuilder;

fn build_insecure_client() -> reqwest::Client {
    ClientBuilder::new()
        .danger_accept_invalid_hostnames(true)
        .build()
        .unwrap()
}
