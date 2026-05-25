//! AI provider abstractions and implementations.
//!
//! The primary public interface is the [`Provider`] trait.  Use
//! [`ProviderFactory::create_from_config`] to obtain a provider instance at
//! runtime.
//!
//! # Module layout
//!
//! | Module              | Contents                                              |
//! |---------------------|-------------------------------------------------------|
//! | [`base`]            | [`Provider`] trait definition                         |
//! | [`types`]           | Shared message, tool, and capability types            |
//! | [`openai`]          | OpenAI chat completions provider                      |
//! | [`anthropic`]       | Anthropic Messages API provider                       |
//! | [`ollama`]          | Ollama local inference provider                       |
//! | [`copilot`]         | GitHub Copilot provider                               |
//! | [`copilot_auth`]    | OAuth device-flow authentication for Copilot          |
//! | [`factory`]         | [`ProviderFactory`] — constructs providers from config|
//! | [`model_resolution`]| [`ModelResolver`] — selects the best model for a task |

pub mod anthropic;
pub mod base;
pub mod copilot;
pub mod copilot_auth;
pub mod factory;
pub mod model_resolution;
pub mod ollama;
pub mod openai;
pub mod types;

pub use base::Provider;
pub use factory::ProviderFactory;
pub use model_resolution::{ModelResolver, ResolutionContext, ResolvedModel};
pub use types::{
    CredentialStatus, FunctionCall, Message, MetadataSource, ModelCapabilities, ModelMetadata,
    ProviderCapabilities, ProviderMetadata, Role, ThinkingMode, Tool, ToolCall,
};
