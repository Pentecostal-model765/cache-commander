pub mod safety;
pub mod tools;

use crate::config::Config;
use crate::providers;
use crate::scanner;
use crate::scanner::walker;
use crate::security;
use crate::tree::node::TreeNode;

use humansize::{format_size, BINARY};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
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

    #[tool(
        description = "Search for packages by name across all caches. Returns matching packages with name, version, size, and safety level."
    )]
    async fn search_packages(
        &self,
        input: Parameters<tools::SearchInput>,
    ) -> Result<String, String> {
        let server = self.clone();
        let input = input.0;
        let query = input.query.to_lowercase();
        let ecosystem = input.ecosystem;

        let result = tokio::task::spawn_blocking(move || {
            let nodes = server.walk_roots();
            let matches: Vec<PackageEntry> = nodes
                .into_iter()
                .filter(|node| {
                    let name_match = node.name.to_lowercase().contains(&query);
                    let eco_match = ecosystem
                        .as_ref()
                        .map_or(true, |eco| node.kind.label().eq_ignore_ascii_case(eco));
                    name_match && eco_match
                })
                .map(|node| {
                    let safety = providers::safety(node.kind, &node.path);
                    PackageEntry {
                        name: node.name.clone(),
                        version: providers::package_id(node.kind, &node.path)
                            .map(|p| p.version)
                            .unwrap_or_default(),
                        ecosystem: node.kind.label().to_string(),
                        path: node.path.to_string_lossy().to_string(),
                        size: format_size(node.size, BINARY),
                        size_bytes: node.size,
                        safety_level: safety.label().to_string(),
                        safety_icon: safety.icon().to_string(),
                    }
                })
                .collect();
            matches
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;

        if result.is_empty() {
            Ok("No packages found matching query.".to_string())
        } else {
            serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failed: {e}"))
        }
    }

    #[tool(description = "Get detailed metadata for a specific cache entry by its absolute path")]
    async fn get_package_details(
        &self,
        input: Parameters<tools::PathInput>,
    ) -> Result<String, String> {
        let input = input.0;
        let path = PathBuf::from(&input.path);
        if !path.exists() {
            return Ok(format!("Path not found: {}", input.path));
        }

        let result = tokio::task::spawn_blocking(move || {
            let kind = providers::detect(&path);
            let safety = providers::safety(kind, &path);
            let size = walker::dir_size(&path);
            let last_modified = path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Local> = t.into();
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                });

            let metadata: Vec<MetadataEntry> = providers::metadata(kind, &path)
                .into_iter()
                .map(|m| MetadataEntry {
                    label: m.label,
                    value: m.value,
                })
                .collect();

            PackageDetails {
                provider: kind.label().to_string(),
                name: providers::semantic_name(kind, &path).unwrap_or_else(|| {
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                }),
                version: providers::package_id(kind, &path)
                    .map(|p| p.version)
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                size: format_size(size, BINARY),
                size_bytes: size,
                last_modified,
                safety_level: safety.label().to_string(),
                safety_icon: safety.icon().to_string(),
                metadata,
            }
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;

        serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failed: {e}"))
    }

    #[tool(
        description = "Scan cached packages for known vulnerabilities (CVEs) via OSV.dev. Returns vulnerable packages with CVE details and fix versions."
    )]
    async fn scan_vulnerabilities(
        &self,
        input: Parameters<tools::EcosystemInput>,
    ) -> Result<String, String> {
        let roots = self.roots.clone();
        let ecosystem_filter = input.0.ecosystem;

        let result = tokio::task::spawn_blocking(move || {
            let mut packages = scanner::discover_packages(&roots);
            if let Some(ref eco) = ecosystem_filter {
                packages.retain(|(_, pkg)| pkg.ecosystem.eq_ignore_ascii_case(eco));
            }
            if packages.is_empty() {
                return Vec::new();
            }
            let vulns = security::scan_vulns(&packages);
            vulns
                .into_iter()
                .map(|(path, info)| {
                    let kind = providers::detect(&path);
                    let pkg_id = providers::package_id(kind, &path);
                    let (name, version, ecosystem) = pkg_id
                        .map(|p| (p.name.clone(), p.version.clone(), p.ecosystem.to_string()))
                        .unwrap_or_else(|| {
                            let name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            (name, String::new(), String::new())
                        });
                    VulnResult {
                        name: name.clone(),
                        version: version.clone(),
                        ecosystem,
                        path: path.to_string_lossy().to_string(),
                        vulnerabilities: info
                            .vulns
                            .into_iter()
                            .map(|v| VulnEntry {
                                id: v.id,
                                summary: v.summary,
                                severity: v.severity,
                                fix_version: v.fix_version.clone(),
                                upgrade_command: providers::upgrade_command(
                                    kind,
                                    &name,
                                    &v.fix_version.unwrap_or(version.clone()),
                                ),
                            })
                            .collect(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;

        if result.is_empty() {
            Ok("No vulnerabilities found.".to_string())
        } else {
            serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failed: {e}"))
        }
    }

    #[tool(
        description = "Check cached packages for available version updates. Returns outdated packages with current and latest versions."
    )]
    async fn check_outdated(
        &self,
        input: Parameters<tools::EcosystemInput>,
    ) -> Result<String, String> {
        let roots = self.roots.clone();
        let ecosystem_filter = input.0.ecosystem;

        let result = tokio::task::spawn_blocking(move || {
            let mut packages = scanner::discover_packages(&roots);
            if let Some(ref eco) = ecosystem_filter {
                packages.retain(|(_, pkg)| pkg.ecosystem.eq_ignore_ascii_case(eco));
            }
            if packages.is_empty() {
                return Vec::new();
            }
            let versions = security::check_versions(&packages);
            versions
                .into_iter()
                .filter(|(_, info)| info.is_outdated)
                .map(|(path, info)| {
                    let kind = providers::detect(&path);
                    let pkg_id = providers::package_id(kind, &path);
                    let (name, ecosystem) = pkg_id
                        .map(|p| (p.name.clone(), p.ecosystem.to_string()))
                        .unwrap_or_else(|| {
                            (
                                path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string(),
                                String::new(),
                            )
                        });
                    OutdatedResult {
                        name: name.clone(),
                        version: info.current,
                        latest: info.latest.clone(),
                        ecosystem,
                        path: path.to_string_lossy().to_string(),
                        upgrade_command: providers::upgrade_command(kind, &name, &info.latest),
                    }
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;

        if result.is_empty() {
            Ok("All packages are up to date.".to_string())
        } else {
            serde_json::to_string_pretty(&result).map_err(|e| format!("serialization failed: {e}"))
        }
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
