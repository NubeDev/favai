//! Minimal MCP stdio server built on the official `rmcp` SDK.
//!
//! Purpose: A/B test against `starter-mcp` to determine whether
//! Claude Code's slash-command picker can see prompts from THIS
//! server. If `/mcp__rmcpdemo__greeting` autocompletes but
//! `/mcp__favai__com_demo_skills_hello` does not, the bug is in
//! starter-mcp's protocol surface. If neither shows up, the bug
//! is in the host / `.mcp.json` wiring.
//!
//! No stdout writes outside of the rmcp transport — banner text
//! would corrupt the JSON-RPC stream.

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::prompt::PromptRouter, wrapper::Parameters},
    model::*,
    prompt, prompt_handler, prompt_router,
    schemars::JsonSchema,
    service::RequestContext,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Greeting parameters")]
pub struct GreetArgs {
    #[schemars(description = "Who to greet")]
    pub name: String,
}

#[derive(Clone)]
pub struct DemoServer {
    prompt_router: PromptRouter<DemoServer>,
}

impl DemoServer {
    pub fn new() -> Self {
        Self {
            prompt_router: Self::prompt_router(),
        }
    }
}

impl Default for DemoServer {
    fn default() -> Self {
        Self::new()
    }
}

#[prompt_router]
impl DemoServer {
    #[prompt(
        name = "greeting",
        description = "A no-parameter greeting prompt — should appear in Claude Code's slash picker as /mcp__rmcpdemo__greeting."
    )]
    async fn greeting(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            "Hello from rmcp-demo. If you can read this, the prompt round-tripped.",
        )]
    }

    #[prompt(
        name = "greet_named",
        description = "A parameterised greeting prompt — proves typed arguments still surface as a slash command."
    )]
    async fn greet_named(
        &self,
        Parameters(args): Parameters<GreetArgs>,
    ) -> Result<GetPromptResult, McpError> {
        Ok(GetPromptResult {
            description: Some(format!("Greeting for {}", args.name)),
            messages: vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!("Hello, {}, from rmcp-demo.", args.name),
            )],
        })
    }
}

#[prompt_handler]
impl ServerHandler for DemoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_prompts().build(),
            instructions: Some("rmcp-demo: minimal A/B test server.".to_string()),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // stderr only — never touch stdout, it belongs to JSON-RPC.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    tracing::info!("rmcp-demo: starting MCP stdio loop");
    let service = DemoServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
