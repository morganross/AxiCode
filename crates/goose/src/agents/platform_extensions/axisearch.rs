//! Web search through an AxiSearch service.
//!
//! Off by default. When enabled it replaces the shell-based `web-search` built-in skill with two
//! native tools that call one AxiSearch service over HTTP, so no Python or POSIX shell is needed
//! and all sessions share the service's quotas and caches.
//!
//! Configuration:
//! - `AXISEARCH_URL`: service base URL, default `http://127.0.0.1:8787`.
//! - `AXISEARCH_TOKEN`: bearer token, when the service requires one.

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "axisearch";
pub const SEARCH_TOOL_NAME: &str = "search";
pub const SEARCH_RAW_TOOL_NAME: &str = "search_raw";

const DEFAULT_URL: &str = "http://127.0.0.1:8787";
/// The service enforces its own, shorter limit; this only guards against a hung connection.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(180);
const RAW_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOOL_TEXT_CHARS: usize = 60_000;
const MAX_RESULTS: u32 = 20;
const DEFAULT_RAW_COUNT: u32 = 10;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchParams {
    /// The question or keywords, up to 512 characters.
    query: String,
    /// How many sources to use, 1 to 20.
    #[serde(default)]
    max_results: Option<u32>,
    /// Also return the full extracted text of the pages.
    #[serde(default)]
    include_page_text: Option<bool>,
    /// Force the scientific-literature pipeline.
    #[serde(default)]
    science: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchRawParams {
    /// Keywords, up to 512 characters.
    query: String,
    /// Number of results, 1 to 20. Defaults to 10.
    #[serde(default)]
    count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SearchReply {
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    results: Vec<ReplyResult>,
}

#[derive(Debug, Deserialize)]
struct ReplyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct RawReply {
    #[serde(default)]
    results: Vec<RawHit>,
}

#[derive(Debug, Deserialize)]
struct RawHit {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    published: Option<String>,
}

pub struct AxiSearchClient {
    info: InitializeResult,
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl AxiSearchClient {
    pub fn new(_context: PlatformExtensionContext) -> Result<Self> {
        let config = Config::global();
        let base_url = config
            .get_param::<String>("AXISEARCH_URL")
            .unwrap_or_default();
        let token = config.get_secret::<String>("AXISEARCH_TOKEN").ok();
        Self::with_endpoint(&base_url, token)
    }

    fn with_endpoint(base_url: &str, token: Option<String>) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("AxiSearch"),
            )
            .with_instructions(
                indoc! {r#"
                Use the search tool for questions that need current or factual information from
                the web or scientific literature. It returns a researched answer with sources.
                Use search_raw when you only need a list of links and snippets.
            "#}
                .to_string(),
            );

        let base_url = normalize_url(base_url);
        let mut http = reqwest::Client::builder().connect_timeout(CONNECT_TIMEOUT);
        if is_loopback(&base_url) {
            // A system proxy must never intercept the local service.
            http = http.no_proxy();
        }
        Ok(Self {
            info,
            base_url,
            token: token.filter(|token| !token.trim().is_empty()),
            http: http.build()?,
        })
    }

    async fn call<R: for<'de> Deserialize<'de>>(
        &self,
        mut request: reqwest::RequestBuilder,
    ) -> Result<R, String> {
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!("AxiSearch service unreachable: {error}");
            "AxiSearch is not reachable. Check that it is enabled and running.".to_string()
        })?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err("AxiSearch returned too much data".to_string());
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| "AxiSearch closed the connection early".to_string())?;
        if body.len() > MAX_RESPONSE_BYTES {
            return Err("AxiSearch returned too much data".to_string());
        }
        if !(200..300).contains(&status) {
            return Err(service_error(status, &String::from_utf8_lossy(&body)));
        }
        serde_json::from_slice(&body)
            .map_err(|_| "AxiSearch returned a response this client could not read".to_string())
    }

    async fn search(&self, params: SearchParams) -> Result<String, String> {
        let body = json!({
            "query": params.query,
            "include_answer": true,
            "include_raw_content": params.include_page_text.unwrap_or(false),
            "max_results": params.max_results.map(|n| n.clamp(1, MAX_RESULTS)),
            "advanced_search": params.science.filter(|science| *science),
            "caller": "axicode",
        });
        let request = self
            .http
            .post(format!("{}/v1/search", self.base_url))
            .timeout(SEARCH_TIMEOUT)
            .json(&body);
        let reply: SearchReply = self.call(request).await?;
        Ok(format_search(&reply))
    }

    async fn search_raw(&self, params: SearchRawParams) -> Result<String, String> {
        let count = params
            .count
            .unwrap_or(DEFAULT_RAW_COUNT)
            .clamp(1, MAX_RESULTS)
            .to_string();
        let url = reqwest::Url::parse_with_params(
            &format!("{}/v1/search/raw", self.base_url),
            &[("q", params.query.as_str()), ("count", count.as_str())],
        )
        .map_err(|_| "AXISEARCH_URL is not a valid URL".to_string())?;
        let request = self.http.get(url).timeout(RAW_TIMEOUT);
        let reply: RawReply = self.call(request).await?;
        Ok(format_raw(&reply))
    }

    async fn dispatch(&self, name: &str, arguments: Option<JsonObject>) -> Result<String, String> {
        let arguments = Value::Object(arguments.unwrap_or_default());
        match name {
            SEARCH_TOOL_NAME => {
                let params = serde_json::from_value(arguments)
                    .map_err(|error| format!("Invalid arguments: {error}"))?;
                self.search(params).await
            }
            SEARCH_RAW_TOOL_NAME => {
                let params = serde_json::from_value(arguments)
                    .map_err(|error| format!("Invalid arguments: {error}"))?;
                self.search_raw(params).await
            }
            _ => Err(format!("Unknown tool: {name}")),
        }
    }

    fn get_tools() -> Vec<Tool> {
        let search = serde_json::to_value(schema_for!(SearchParams))
            .expect("Failed to serialize SearchParams schema");
        let raw = serde_json::to_value(schema_for!(SearchRawParams))
            .expect("Failed to serialize SearchRawParams schema");
        vec![
            Tool::new(
                SEARCH_TOOL_NAME.to_string(),
                indoc! {r#"
                    Research a question on the web, and in scientific sources when the question is
                    technical. Reads the best pages and returns a written answer followed by the
                    source material with URLs. Takes 10 to 60 seconds. Use it when you need an
                    answer, not just links.
                "#}
                .to_string(),
                search.as_object().unwrap().clone(),
            )
            .annotate(read_only("Search the web")),
            Tool::new(
                SEARCH_RAW_TOOL_NAME.to_string(),
                indoc! {r#"
                    Return web search results (title, URL, snippet) without reading pages or
                    writing an answer. Fast. Use it to find links.
                "#}
                .to_string(),
                raw.as_object().unwrap().clone(),
            )
            .annotate(read_only("List web results")),
        ]
    }
}

fn read_only(title: &str) -> ToolAnnotations {
    ToolAnnotations::from_raw(
        Some(title.to_string()),
        Some(true),
        Some(false),
        Some(true),
        Some(true),
    )
}

#[async_trait]
impl McpClientTrait for AxiSearchClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let outcome = tokio::select! {
            outcome = self.dispatch(name, arguments) => outcome,
            () = cancellation_token.cancelled() => Err("Search cancelled".to_string()),
        };
        match outcome {
            Ok(text) => Ok(CallToolResult::success(vec![ContentBlock::text(text)])),
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error: {error}"
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

fn normalize_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        DEFAULT_URL.to_string()
    } else {
        url.to_string()
    }
}

fn is_loopback(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]"))
}

fn format_search(reply: &SearchReply) -> String {
    let mut out = String::new();
    if let Some(answer) = reply.answer.as_deref().filter(|a| !a.trim().is_empty()) {
        out.push_str("Answer:\n");
        out.push_str(answer.trim());
        out.push_str("\n\n");
    }
    for result in &reply.results {
        if !result.title.is_empty() {
            out.push_str("## ");
            out.push_str(&result.title);
            out.push('\n');
        }
        if result.url.starts_with("http") {
            out.push_str(&result.url);
            out.push('\n');
        }
        out.push_str(result.content.trim());
        out.push_str("\n\n");
    }
    if out.trim().is_empty() {
        return "No results.".to_string();
    }
    truncate(out.trim_end())
}

fn format_raw(reply: &RawReply) -> String {
    if reply.results.is_empty() {
        return "No results.".to_string();
    }
    let mut out = String::new();
    for (index, hit) in reply.results.iter().enumerate() {
        out.push_str(&format!("{}. {}\n{}\n", index + 1, hit.title, hit.url));
        if let Some(date) = hit.published.as_deref().filter(|date| !date.is_empty()) {
            out.push_str(&format!("Published: {date}\n"));
        }
        if !hit.snippet.trim().is_empty() {
            out.push_str(hit.snippet.trim());
            out.push('\n');
        }
        out.push('\n');
    }
    truncate(out.trim_end())
}

/// Turns a failed service reply into a short message for the model.
fn service_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("detail")
            .or_else(|| value.get("error"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let reason = match status {
        401 | 403 => "AxiSearch rejected the configured token",
        429 => "AxiSearch is rate limited; try again shortly",
        503 => "AxiSearch has no working provider right now",
        504 => "the search timed out",
        _ => "the search failed",
    };
    match detail {
        Some(detail) if !detail.is_empty() => format!("{reason} ({status}): {detail}"),
        _ => format!("{reason} ({status})"),
    }
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_TOOL_TEXT_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_TOOL_TEXT_CHARS).collect();
    format!("{kept}\n\n[truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer, token: Option<&str>) -> AxiSearchClient {
        AxiSearchClient::with_endpoint(&server.uri(), token.map(str::to_owned)).unwrap()
    }

    fn arguments(value: Value) -> Option<JsonObject> {
        value.as_object().cloned()
    }

    #[tokio::test]
    async fn search_sends_token_and_formats_answer_with_sources() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/search"))
            .and(header("authorization", "Bearer tok"))
            .and(body_partial_json(json!({
                "query": "what is rust", "include_answer": true, "max_results": 20, "caller": "axicode"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "answer": "Rust is a language.",
                "results": [{"title": "Context", "url": "https://example.com/a", "content": "Source text."}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let text = client(&server, Some("tok"))
            .dispatch(
                SEARCH_TOOL_NAME,
                arguments(json!({"query": "what is rust", "max_results": 50})),
            )
            .await
            .unwrap();

        assert!(text.starts_with("Answer:\nRust is a language."));
        assert!(text.contains("https://example.com/a"));
        assert!(text.contains("Source text."));
    }

    #[tokio::test]
    async fn search_raw_lists_hits_and_clamps_count() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search/raw"))
            .and(query_param("q", "tokio"))
            .and(query_param("count", "20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [{"title": "Tokio", "url": "https://tokio.rs", "snippet": "A runtime."}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let text = client(&server, None)
            .dispatch(
                SEARCH_RAW_TOOL_NAME,
                arguments(json!({"query": "tokio", "count": 99})),
            )
            .await
            .unwrap();

        assert_eq!(text, "1. Tokio\nhttps://tokio.rs\nA runtime.");
    }

    #[tokio::test]
    async fn failures_reach_the_model_as_short_messages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search/raw"))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_json(json!({"detail": "all providers cooling"})),
            )
            .mount(&server)
            .await;
        let client = client(&server, None);

        let error = client
            .dispatch(SEARCH_RAW_TOOL_NAME, arguments(json!({"query": "x"})))
            .await
            .unwrap_err();
        assert!(error.contains("503") && error.contains("all providers cooling"));

        let missing = client
            .dispatch(SEARCH_TOOL_NAME, arguments(json!({})))
            .await
            .unwrap_err();
        assert!(missing.starts_with("Invalid arguments"));
        assert!(client.dispatch("other", None).await.is_err());
    }

    #[test]
    fn urls_default_and_normalize() {
        assert_eq!(normalize_url("  "), DEFAULT_URL);
        assert_eq!(normalize_url("http://host:1/"), "http://host:1");
        assert!(is_loopback(DEFAULT_URL));
        assert!(!is_loopback("https://search.example.com"));
    }
}
