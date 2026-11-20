use reqwest::ClientBuilder;

fn build_secure_client() -> reqwest::Client {
    ClientBuilder::new().build().unwrap()
}
