# XZardgz Architecture

## Overview

XZardgz is an autonomous AI agent CLI application written in Rust that executes workflows from structured plans. It connects to AI providers (GitHub Copilot or Ollama) to perform repository analysis and automated documentation generation following the Diataxis framework.

## System Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        XZardgz CLI                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐      ┌─────────────┐   ┌──────────────┐ │
│  │   CLI Layer  │────▶ │ Agent Core  │──▶│  Workflow    │ │
│  │   (clap)     │      │             │   │  Engine      │ │
│  └──────────────┘      └─────────────┘   └──────────────┘ │
│                               │                            │
│                               ▼                            │
│  ┌──────────────────────────────────────────────────────┐ │
│  │           Provider Abstraction Layer                 │ │
│  │  ┌─────────────────┐      ┌─────────────────┐       │ │
│  │  │ Copilot Provider│      │ Ollama Provider │       │ │
│  │  └─────────────────┘      └─────────────────┘       │ │
│  └──────────────────────────────────────────────────────┘ │
│                               │                            │
│                               ▼                            │
│  ┌──────────────────────────────────────────────────────┐ │
│  │          Repository Analysis & Tools                 │ │
│  │  ┌──────────┐  ┌──────────┐  ┌─────────────┐       │ │
│  │  │ Git Scan │  │ File I/O │  │ Doc Gen     │       │ │
│  │  └──────────┘  └──────────┘  └─────────────┘       │ │
│  └──────────────────────────────────────────────────────┘ │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. CLI Layer

**Purpose**: User interface and command routing

**Responsibilities**:
- Parse command-line arguments and flags
- Validate input parameters
- Display progress and results
- Handle interactive prompts (optional)
- Load and parse plan files (JSON, YAML, Markdown)

**Key Modules**:
- `cli.rs` - CLI parser using `clap`
- `commands/` - Command handlers
- `config.rs` - Configuration management

**Dependencies**:
- `clap` - Command-line argument parsing
- `serde` - Serialization/deserialization
- `serde_json`, `serde_yaml` - Format support

### 2. Agent Core

**Purpose**: Orchestrate AI-powered autonomous execution

**Responsibilities**:
- Manage agent lifecycle
- Maintain conversation context
- Execute agent decision loop
- Handle error recovery
- Track execution state

**Key Modules**:
- `agent/mod.rs` - Main agent implementation
- `agent/state.rs` - State management
- `agent/context.rs` - Conversation context
- `agent/executor.rs` - Execution engine

**Architecture Pattern**: Based on goose Agent pattern
```rust
pub struct Agent {
    provider: Arc<dyn Provider>,
    context: ConversationContext,
    tools: Vec<Tool>,
    config: AgentConfig,
}
```

### 3. Provider Abstraction Layer

**Purpose**: Unified interface to AI providers

**Responsibilities**:
- Abstract provider-specific APIs
- Handle authentication
- Manage streaming responses
- Implement retry logic
- Track token usage

**Key Modules**:
- `providers/mod.rs` - Provider trait definition
- `providers/copilot.rs` - GitHub Copilot integration
- `providers/ollama.rs` - Ollama integration
- `providers/factory.rs` - Provider instantiation

**Provider Trait**:
```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn metadata() -> ProviderMetadata;
    fn get_name(&self) -> &str;
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<Tool>,
    ) -> Result<Message, ProviderError>;
}
```

### 4. Workflow Engine

**Purpose**: Execute structured plans and workflows

**Responsibilities**:
- Parse plan documents (JSON, YAML, Markdown)
- Validate plan structure
- Execute workflow steps sequentially
- Handle conditional logic
- Report progress and results

**Key Modules**:
- `workflow/mod.rs` - Workflow engine
- `workflow/plan.rs` - Plan data structures
- `workflow/executor.rs` - Step execution
- `workflow/parser.rs` - Plan parsing

**Plan Structure**:
```rust
pub struct Plan {
    pub name: String,
    pub description: String,
    pub steps: Vec<WorkflowStep>,
    pub deliverables: Vec<Deliverable>,
}

pub struct WorkflowStep {
    pub id: String,
    pub description: String,
    pub action: Action,
    pub dependencies: Vec<String>,
}
```

### 5. Repository Analysis

**Purpose**: Scan and analyze repository contents

**Responsibilities**:
- Clone/access Git repositories
- Traverse directory structure
- Read and parse source files
- Extract code metadata
- Generate content summaries

**Key Modules**:
- `repository/mod.rs` - Repository interface
- `repository/scanner.rs` - File system scanner
- `repository/git.rs` - Git operations
- `repository/analyzer.rs` - Content analysis

**Dependencies**:
- `git2` - Git operations
- `ignore` - .gitignore handling
- `walkdir` - Directory traversal

### 6. Documentation Generator

**Purpose**: Generate Diataxis-compliant documentation

**Responsibilities**:
- Generate tutorial documentation
- Create how-to guides
- Write explanation documents
- Generate reference documentation
- Organize output structure

**Key Modules**:
- `docgen/mod.rs` - Documentation generator
- `docgen/diataxis.rs` - Diataxis framework
- `docgen/templates.rs` - Document templates
- `docgen/writer.rs` - File output

**Diataxis Categories**:
```rust
pub enum DocCategory {
    Tutorial,      // Learning-oriented
    HowTo,         // Task-oriented
    Explanation,   // Understanding-oriented
    Reference,     // Information-oriented
}

pub struct DocTemplate {
    pub category: DocCategory,
    pub title: String,
    pub sections: Vec<Section>,
}
```

## Data Flow

### Primary Workflow: Repository Documentation Generation

```
1. User Input
   ↓
2. Parse Plan (JSON/YAML/Markdown)
   ↓
3. Initialize Agent with Provider
   ↓
4. Execute Workflow Steps:
   a. Clone/Access Repository
   b. Scan Repository Contents
   c. Analyze Code Structure
   d. Generate Documentation Outline
   e. Generate Content (via AI Provider)
   f. Write Documentation Files
   ↓
5. Validate & Report Results
```

### Agent Execution Loop

```
1. Receive User Message/Instruction
   ↓
2. Build Context (system prompt + conversation history)
   ↓
3. Query AI Provider with Tools
   ↓
4. Process Response:
   - If tool call → Execute tool → Add result to context → Loop
   - If final response → Return to user
   ↓
5. Update Conversation History
```

## Module Structure

```
xzardgz/
├── src/
│   ├── main.rs                 # Entry point
│   ├── lib.rs                  # Library root
│   ├── cli.rs                  # CLI parser
│   ├── config.rs               # Configuration
│   ├── error.rs                # Error types
│   │
│   ├── agent/                  # Agent core
│   │   ├── mod.rs
│   │   ├── agent.rs           # Main agent
│   │   ├── context.rs         # Conversation context
│   │   ├── state.rs           # Agent state
│   │   └── executor.rs        # Execution logic
│   │
│   ├── providers/              # AI providers
│   │   ├── mod.rs
│   │   ├── base.rs            # Provider trait
│   │   ├── copilot.rs         # GitHub Copilot
│   │   ├── ollama.rs          # Ollama
│   │   ├── factory.rs         # Provider factory
│   │   └── types.rs           # Shared types
│   │
│   ├── workflow/               # Workflow engine
│   │   ├── mod.rs
│   │   ├── plan.rs            # Plan structures
│   │   ├── parser.rs          # Plan parsing
│   │   ├── executor.rs        # Execution
│   │   └── validator.rs       # Validation
│   │
│   ├── repository/             # Repository operations
│   │   ├── mod.rs
│   │   ├── scanner.rs         # File scanning
│   │   ├── git.rs             # Git operations
│   │   └── analyzer.rs        # Analysis
│   │
│   ├── docgen/                 # Documentation generation
│   │   ├── mod.rs
│   │   ├── diataxis.rs        # Framework
│   │   ├── generator.rs       # Content generation
│   │   ├── templates.rs       # Templates
│   │   └── writer.rs          # File writing
│   │
│   └── tools/                  # Agent tools
│       ├── mod.rs
│       ├── file_ops.rs        # File operations
│       ├── git_ops.rs         # Git operations
│       └── doc_ops.rs         # Documentation ops
│
├── tests/                      # Integration tests
├── examples/                   # Usage examples
└── docs/                       # Documentation
    ├── tutorials/
    ├── how_to/
    ├── explanation/
    └── reference/
```

## Configuration

### Configuration Sources (Priority Order)

1. Command-line arguments
2. Environment variables
3. Configuration file (`~/.config/xzardgz/config.yaml`)
4. Default values

### Configuration Structure

```yaml
# ~/.config/xzardgz/config.yaml

# AI Provider settings
provider:
  type: copilot  # or 'ollama'

  # Copilot-specific
  copilot:
    model: gpt-4o

  # Ollama-specific
  ollama:
    host: localhost:11434
    model: qwen3

# Agent settings
agent:
  max_turns: 50
  timeout_seconds: 600
  retry_attempts: 3

# Repository settings
repository:
  clone_depth: 1
  ignore_patterns:
    - node_modules
    - target
    - .git

# Documentation settings
documentation:
  output_dir: docs
  categories:
    - tutorials
    - how_to
    - explanation
    - reference
```

## Error Handling

### Error Types Hierarchy

```rust
#[derive(Debug, thiserror::Error)]
pub enum XzardgzError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Workflow error: {0}")]
    Workflow(#[from] WorkflowError),

    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Error Handling Strategy

1. **Recoverable Errors**: Retry with exponential backoff
2. **User Errors**: Provide clear error messages and suggestions
3. **System Errors**: Log detailed information and fail gracefully
4. **Provider Errors**: Fallback or alternative approaches

## Security Considerations

### Authentication

- **GitHub Copilot**: OAuth 2.0 device flow
- **Ollama**: Local or authenticated endpoint

### Data Privacy

- No sensitive data logged
- Credentials stored securely (system keychain)
- Repository content processed locally when possible

### Input Validation

- Sanitize all user inputs
- Validate plan files before execution
- Restrict file system access to designated paths

## Performance Considerations

### Optimization Strategies

1. **Streaming Responses**: Process AI responses as they arrive
2. **Concurrent Operations**: Parallel file scanning and analysis
3. **Caching**: Cache repository metadata and analysis results
4. **Token Management**: Optimize prompts to minimize token usage

### Resource Limits

- Maximum conversation context: 100,000 tokens
- Maximum file size for analysis: 1 MB
- Repository size limit: 1 GB (configurable)
- Concurrent operations: 4 (configurable)

## Testing Strategy

### Test Categories

1. **Unit Tests**: Individual component testing
2. **Integration Tests**: Component interaction testing
3. **End-to-End Tests**: Full workflow testing
4. **Provider Tests**: Mock provider responses

### Test Coverage Goals

- Overall coverage: >80%
- Critical paths: >95%
- Error handling: 100%

### Test Organization

```
tests/
├── unit/
│   ├── agent_tests.rs
│   ├── provider_tests.rs
│   └── workflow_tests.rs
├── integration/
│   ├── cli_tests.rs
│   └── workflow_tests.rs
└── e2e/
    └── documentation_generation_tests.rs
```

## Deployment

### Build Configuration

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

### Distribution

- Single statically-linked binary
- Platform-specific releases (Linux, macOS, Windows)
- Container image (optional)

### Installation Methods

1. Binary download
2. Cargo install
3. Package managers (homebrew, apt, etc.)

## Observability

### Logging

- Structured logging using `tracing`
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Log output: stderr (configurable)

### Metrics

- Execution time per workflow step
- Token usage per operation
- Success/failure rates
- Provider response times

### Health Checks

- Provider connectivity
- File system access
- Configuration validity

## Dependencies

### Core Dependencies

- `clap` - CLI argument parsing
- `tokio` - Async runtime
- `serde` - Serialization
- `serde_json`, `serde_yaml` - Format support
- `anyhow`, `thiserror` - Error handling
- `tracing` - Logging

### Provider Dependencies

- `reqwest` - HTTP client
- `async-trait` - Async trait support

### Repository Dependencies

- `git2` - Git operations
- `ignore` - Gitignore support
- `walkdir` - Directory traversal

### Documentation Dependencies

- `regex` - Pattern matching
- `handlebars` - Template engine (optional)

## Future Extensibility

### Planned Extensions

1. **Additional Providers**: OpenAI, Anthropic, Azure OpenAI
2. **Plan Formats**: TOML, custom DSL
3. **Output Formats**: PDF, HTML, Confluence
4. **CI/CD Integration**: GitHub Actions, GitLab CI
5. **Web Interface**: Optional web UI for monitoring

### Plugin System (Future)

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn initialize(&mut self, config: &Config) -> Result<()>;
    fn process(&self, context: &mut Context) -> Result<()>;
}
```

## References

### Inspiration Sources

- **goose**: Agent architecture, provider abstraction
- **Zed Agent**: Tool integration, conversation management
- **Zed Copilot**: Authentication flows
- **Zed Ollama**: Local provider integration

### Standards & Frameworks

- **Diataxis**: Documentation framework
- **SPDX**: License specification
- **RFC 3339**: Timestamp format
- **ULID**: Unique identifiers
- **OpenAPI**: API documentation

## Conclusion

XZardgz is designed as a modular, extensible autonomous AI agent for repository documentation generation. The architecture emphasizes:

- **Simplicity**: Clear separation of concerns
- **Reliability**: Comprehensive error handling and testing
- **Performance**: Async operations and resource optimization
- **Extensibility**: Plugin-ready design for future enhancements
- **Standards Compliance**: Following best practices and industry standards

The phased implementation approach will build this architecture incrementally, starting with core functionality and progressively adding features.
