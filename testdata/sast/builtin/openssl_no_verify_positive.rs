use openssl::ssl::{SslContext, SslMethod, SslVerifyMode};

fn insecure_context() -> SslContext {
    let mut ctx = SslContext::builder(SslMethod::tls()).unwrap();
    ctx.set_verify(SslVerifyMode::NONE);
    ctx.build()
}
