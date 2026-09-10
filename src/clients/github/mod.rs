//! GitHub API clients.
//!
//! Provides a REST API client for GitHub pull request creation.
//! GitHub is the only supported host; GitLab is explicitly out of scope.
//!
//! # Component-Boundary Contract
//!
//! | Rule           | Detail                                               |
//! |----------------|------------------------------------------------------|
//! | May depend on  | `auth`, `config`                                     |
//! | Must NOT call  | `scanner`, `providers`, `agent`                      |
//! | Must NOT be    | called from `tools/`                                 |

pub mod pr;

pub use pr::{GithubPrClient, PrClientError, PrInput, PrOutput};
