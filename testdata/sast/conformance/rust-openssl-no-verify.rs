// ruleid: rust-openssl-no-verify
ctx.set_verify(SslVerifyMode::NONE);
// ok: verifying peer certificates is safe
ctx.set_verify(SslVerifyMode::PEER);
