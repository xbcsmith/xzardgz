// ruleid: rust-reqwest-invalid-hostnames
let _ = builder.danger_accept_invalid_hostnames(true);
// ok: building without disabling hostname verification is safe
let _ = builder.build();
