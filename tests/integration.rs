//! Phase 19 integration test harness.
//!
//! This file is the Cargo integration-test entry point for the `integration`
//! test suite.  Each sub-module is compiled from the corresponding file in
//! `tests/integration/`.

#[path = "integration/auth_tests.rs"]
mod auth_tests;

#[path = "integration/mcp_tests.rs"]
mod mcp_tests;

#[path = "integration/watcher_tests.rs"]
mod watcher_tests;

#[path = "integration/workflow_tests.rs"]
mod workflow_tests;

#[path = "integration/sarif_tests.rs"]
mod sarif_tests;
