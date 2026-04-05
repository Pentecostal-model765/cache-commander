# MCP Server

cache-explorer exposes an MCP (Model Context Protocol) server that lets AI assistants query and manage developer caches. Through this interface, tools like Claude Code can inspect cache sizes, search for packages, check for vulnerabilities and outdated versions, and selectively delete cache entries — all without leaving the conversation.

## Starting the server

```bash
ccmd mcp
```

The binary must be built with the `mcp` feature enabled (see below).

## Configuring in Claude Code

Add the following to your Claude Code MCP settings:

```json
{
  "mcpServers": {
    "ccmd": {
      "command": "ccmd",
      "args": ["mcp"]
    }
  }
}
```

## Available tools

| Tool | Description |
|------|-------------|
| `list_caches` | List all cache directories with size per provider |
| `get_summary` | High-level dashboard of total cache usage |
| `search_packages` | Find packages by name with optional ecosystem filter |
| `get_package_details` | Full metadata for a specific cache entry |
| `scan_vulnerabilities` | Check for known CVEs via OSV.dev |
| `check_outdated` | Check for available version updates |
| `preview_delete` | Dry-run showing what would be deleted |
| `delete_packages` | Delete cache entries with safety enforcement |

## Safety levels

`delete_packages` enforces a three-tier safety system:

- **Safe** — entries are deleted directly.
- **Caution** — entries require `confirm_caution=true` to proceed.
- **Unsafe** — entries are rejected; use the TUI instead.

This prevents accidental deletion of entries that may be shared, actively in use, or otherwise risky to remove through an automated interface.

## Building with MCP support

MCP support is an optional feature and must be enabled at compile time:

```bash
# Install from crates.io
cargo install ccmd --features mcp

# Or build locally
cargo build --features mcp
```
