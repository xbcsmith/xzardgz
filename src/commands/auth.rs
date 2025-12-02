use crate::error::XzardgzError;
use crate::providers::copilot_auth::CopilotAuth;

pub async fn login() -> Result<(), XzardgzError> {
    println!("Authenticating with GitHub Copilot...");
    let auth = CopilotAuth::new()?;
    let token = auth.get_token().await?;
    println!("Successfully authenticated! Token: {}...", &token[0..10]);
    Ok(())
}
