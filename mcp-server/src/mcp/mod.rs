pub mod handlers;
pub mod http;
pub mod stdio;
pub mod tooling;

// Preserve existing call sites: cortex_scout::mcp::{list_tools, call_tool}
pub use http::{
    call_tool, list_tools, McpCallRequest, McpCallResponse, McpContent, McpTool, McpToolsResponse,
};
