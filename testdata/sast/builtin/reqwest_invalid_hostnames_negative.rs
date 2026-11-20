use reqwest::ClientBuilder;

fn build_tls_client() -> reqwest::Client {
    ClientBuilder::new().use_rustls_tls().build().unwrap()
}
