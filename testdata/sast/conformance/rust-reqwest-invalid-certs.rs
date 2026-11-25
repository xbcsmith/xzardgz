// ruleid: rust-reqwest-invalid-certs
let _ = builder.danger_accept_invalid_certs(true);
// ok: building without disabling certificate verification is safe
let _ = builder.build();
