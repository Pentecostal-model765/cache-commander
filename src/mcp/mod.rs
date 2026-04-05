pub mod safety;
pub mod tools;

use crate::config::Config;
use crate::providers;
use crate::scanner::walker;
use crate::tree::node::TreeNode;

use humansize::{format_size, BINARY};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::serde_json;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use rmcp::ServerHandler;
use rmcp::ServiceExt;
use std::collections::HashMap;
use std::path::PathBuf;

use tools::*;

#[derive(Clone)]
pub struct CcmdMcp {
    roots: Vec<PathBuf>,
    tool_router: ToolRouter<Self>,
}

impl CcmdMcp {
    fn new(config: &Config) -> Self {
        let tool_router = Self::tool_router();
        Self {
            roots: config.roots.clone(),
            tool_router,
        }
    }

    /// Walk all roots and collect TreeNodes for immediate children
    fn walk_roots(&self) -> Vec<TreeNode> {
        let mut nodes = Vec::new();
        for root in &self.roots {
            for child_path in walker::list_children(root) {
                let mut node = TreeNode::new(child_path.clone(), 1, None);
                node.kind = providers::detect(&child_path);
                node.size = walker::dir_size(&child_path);
                if let Some(name) = providers::semantic_name(node.kind, &child_path) {
                    node.name = name;
                }
                nodes.push(node);
            }
        }
        nodes
    }

    fn build_list_caches(&self) -> Vec<CacheRoot> {
        let nodes = self.walk_roots();
        let mut by_provider: HashMap<String, (u64, usize, PathBuf)> = HashMap::new();
        for node in &nodes {
            let label = node.kind.label().to_string();
            let entry = by_provider.entry(label).or_insert((
                0,
                0,
                node.path.parent().unwrap_or(&node.path).to_path_buf(),
            ));
            entry.0 += node.size;
            entry.1 += 1;
        }
        let mut roots: Vec<CacheRoot> = by_provider
            .into_iter()
            .map(|(provider, (size, count, path))| CacheRoot {
                provider,
                path: path.to_string_lossy().to_string(),
                total_size: format_size(size, BINARY),
                total_size_bytes: size,
                item_count: count,
            })
            .collect();
        roots.sort_by(|a, b| b.total_size_bytes.cmp(&a.total_size_bytes));
        roots
    }

    fn build_summary(&self) -> Summary {
        let nodes = self.walk_roots();
        let mut total_size: u64 = 0;
        let mut by_provider: HashMap<String, (u64, usize)> = HashMap::new();
        let mut safe_count = 0usize;
        let mut caution_count = 0usize;
        let mut unsafe_count = 0usize;

        for node in &nodes {
            total_size += node.size;
            let label = node.kind.label().to_string();
            let entry = by_provider.entry(label).or_insert((0, 0));
            entry.0 += node.size;
            entry.1 += 1;

            match providers::safety(node.kind, &node.path) {
                providers::SafetyLevel::Safe => safe_count += 1,
                providers::SafetyLevel::Caution => caution_count += 1,
                providers::SafetyLevel::Unsafe => unsafe_count += 1,
            }
        }

        let mut provider_summaries: Vec<ProviderSummary> = by_provider
            .into_iter()
            .map(|(name, (size, count))| ProviderSummary {
                name,
                size: format_size(size, BINARY),
                size_bytes: size,
                item_count: count,
            })
            .collect();
        provider_summaries.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

        Summary {
            total_size: format_size(total_size, BINARY),
            total_size_bytes: total_size,
            providers: provider_summaries,
            safety_counts: SafetyCounts {
                safe: safe_count,
                caution: caution_count,
                r#unsafe: unsafe_count,
            },
            total_items: nodes.len(),
        }
    }
}

#[tool_router]
impl CcmdMcp {
    #[tool(description = "List all cache directories with size and item count per provider")]
    async fn list_caches(&self) -> Result<String, String> {
        let server = self.clone();
        let result = tokio::task::spawn_blocking(move || server.build_list_caches())
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failed: {e}"))
    }

    #[tool(
        description = "Get a high-level summary of all caches: total size, breakdown by provider, safety level counts"
    )]
    async fn get_summary(&self) -> Result<String, String> {
        let server = self.clone();
        let result = tokio::task::spawn_blocking(move || server.build_summary())
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failed: {e}"))
    }
}

#[tool_handler]
impl ServerHandler for CcmdMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(Default::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "ccmd".to_string(),
                title: Some("Cache Commander".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                description: Some(
                    "Cache Commander MCP server. Browse developer caches, scan for \
                     vulnerabilities, check for outdated packages, and safely clean up \
                     disk space."
                        .to_string(),
                ),
                ..Default::default()
            },
            instructions: Some(
                "Use list_caches or get_summary to start, then scan_vulnerabilities or \
                 check_outdated for security analysis, and delete_packages for cleanup."
                    .to_string(),
            ),
        }
    }
}

pub fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("ccmd=info".parse().unwrap()),
            )
            .init();

        let server = CcmdMcp::new(&config);
        let transport = rmcp::transport::io::stdio();
        let handle = server.serve(transport).await?;
        handle.waiting().await?;
        Ok(())
    })
}
