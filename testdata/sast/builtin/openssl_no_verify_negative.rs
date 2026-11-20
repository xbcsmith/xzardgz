use openssl::ssl::{SslContext, SslMethod, SslVerifyMode};

fn secure_context() -> SslContext {
    let mut ctx = SslContext::builder(SslMethod::tls()).unwrap();
    ctx.set_verify(SslVerifyMode::PEER);
    ctx.build()
}
