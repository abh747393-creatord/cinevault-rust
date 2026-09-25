use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use moviebox_tui::providers::ReleaseProvider;
use moviebox_tui::providers::models::{
    CatalogItem, MediaDetails, ProviderError, ProviderKind, Release,
};
use moviebox_tui::service::MovieBoxService;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone, Debug)]
pub struct StreamTicket {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub created_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<MovieBoxService>,
    pub tickets: Arc<RwLock<HashMap<String, StreamTicket>>>,
    pub http_client: reqwest::Client,
    pub server_port: u16,
    pub public_url: Option<String>,
}

impl AppState {
    pub fn base_url(&self, headers: Option<&HeaderMap>) -> String {
        if let Some(ref pub_url) = self.public_url {
            if !pub_url.is_empty() {
                return pub_url.trim_end_matches('/').to_string();
            }
        }

        if let Some(h) = headers {
            if let Some(host) = h.get("x-forwarded-host").or_else(|| h.get("host")) {
                if let Ok(host_str) = host.to_str() {
                    let proto = h
                        .get("x-forwarded-proto")
                        .and_then(|p| p.to_str().ok())
                        .unwrap_or(
                            if host_str.starts_with("localhost")
                                || host_str.starts_with("127.0.0.1")
                            {
                                "http"
                            } else {
                                "https"
                            },
                        );
                    return format!("{proto}://{host_str}");
                }
            }
        }

        format!("http://localhost:{}", self.server_port)
    }
}

#[derive(Deserialize)]
pub struct SuggestQuery {
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub provider: Option<String>,
    pub page: Option<usize>,
}

#[derive(Deserialize)]
pub struct HomepageQuery {
    pub tab: Option<String>,
    pub page: Option<usize>,
}

#[derive(Deserialize)]
pub struct DetailsQuery {
    pub provider: Option<String>,
}

#[derive(Deserialize)]
pub struct StreamsQuery {
    pub provider: Option<String>,
    pub season: Option<usize>,
    pub episode: Option<usize>,
}

#[derive(Deserialize)]
pub struct TicketQuery {
    pub ticket: String,
    pub file: Option<String>,
}

#[derive(Deserialize)]
pub struct SubtitlesQuery {
    pub resource_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SubtitleProxyQuery {
    pub url: String,
}

#[derive(Serialize)]
pub struct SimpleHealthResponse {
    pub status: String,
    pub version: String,
    pub service: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub providers: Vec<String>,
}

#[derive(Serialize)]
pub struct HomepageResponse {
    pub items: Vec<CatalogItem>,
    pub metrics: HashMap<String, moviebox_tui::models::BrowseMetrics>,
}

#[derive(Serialize, Clone)]
pub struct WebSourceMirror {
    pub label: String,
    pub resolver_url: String,
    pub proxy_url: String,
    pub direct_file: bool,
}

#[derive(Serialize, Clone)]
pub struct WebRelease {
    pub provider: ProviderKind,
    pub filename: String,
    pub quality: Option<String>,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub size_bytes: Option<u64>,
    pub season: Option<usize>,
    pub episode: Option<usize>,
    pub mirrors: Vec<WebSourceMirror>,
    pub resource_id: Option<String>,
}

#[derive(Serialize)]
pub struct WebSubtitleOption {
    pub name: String,
    pub url: String,
    pub proxy_url: String,
}

fn parse_provider(kind_str: Option<&str>) -> ProviderKind {
    match kind_str.unwrap_or("moviebox").to_ascii_lowercase().as_str() {
        "moviebox" | "mb" => ProviderKind::MovieBox,
        "fourkhdhub" | "4k" => ProviderKind::FourKHdHub,
        "circleftp" | "circle" => ProviderKind::BdixCircleFtp,
        "dhakaflix" => ProviderKind::BdixDhakaFlix,
        "addons" => ProviderKind::Addons,
        _ => ProviderKind::MovieBox,
    }
}

async fn simple_health_handler() -> Json<SimpleHealthResponse> {
    Json(SimpleHealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        service: "cinevault_server".to_string(),
    })
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        providers: vec![
            "moviebox".to_string(),
            "fourkhdhub".to_string(),
            "circleftp".to_string(),
            "dhakaflix".to_string(),
            "addons".to_string(),
        ],
    })
}

async fn providers_handler(State(state): State<AppState>) -> impl IntoResponse {
    let providers = [
        ProviderKind::MovieBox,
        ProviderKind::FourKHdHub,
        ProviderKind::BdixCircleFtp,
        ProviderKind::BdixDhakaFlix,
        ProviderKind::Addons,
    ];

    let list: Vec<_> = providers
        .iter()
        .map(|&p| {
            let caps = state.service.capabilities(p);
            serde_json::json!({
                "id": format!("{p:?}").to_lowercase(),
                "name": format!("{p:?}"),
                "capabilities": {
                    "supports_search": caps.supports_search,
                    "supports_pagination": caps.supports_pagination,
                    "supports_series": caps.supports_series,
                    "supports_subtitles": caps.supports_subtitles,
                    "supports_homepage": caps.supports_homepage,
                }
            })
        })
        .collect();

    Json(list)
}

async fn suggest_handler(
    State(state): State<AppState>,
    Query(query): Query<SuggestQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }
    match state.service.suggest(&q).await {
        Ok(suggestions) => Ok(Json(suggestions)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn search_handler(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<CatalogItem>>, (StatusCode, String)> {
    let provider = parse_provider(query.provider.as_deref());
    let page = query.page.unwrap_or(1);
    match state.service.search_typed(provider, &query.q, page).await {
        Ok(items) => Ok(Json(items)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn homepage_handler(
    State(state): State<AppState>,
    Query(query): Query<HomepageQuery>,
) -> Result<Json<HomepageResponse>, (StatusCode, String)> {
    let tab_str = query.tab.as_deref().unwrap_or("2");
    let tab_id = match tab_str {
        "all" | "featured" => "2",
        "movie" | "movies" => "1",
        "tv" | "series" => "3",
        other => other,
    };
    let page = query.page.unwrap_or(1);
    match state.service.homepage(tab_id, page).await {
        Ok((items, metrics)) => Ok(Json(HomepageResponse { items, metrics })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

fn clean_id(id: &str) -> &str {
    id.strip_prefix("mb-")
        .or_else(|| id.strip_prefix("c-"))
        .unwrap_or(id)
}

async fn debug_play_info_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let clean_id = clean_id(&id);
    let probe = state.service.client.debug_probe(clean_id).await;
    Ok(Json(probe))
}

async fn details_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DetailsQuery>,
) -> Result<Json<MediaDetails>, (StatusCode, String)> {
    let provider = parse_provider(query.provider.as_deref());
    let clean_id = clean_id(&id);
    match state.service.details_typed(provider, clean_id).await {
        Ok(details) => Ok(Json(details)),
        Err(ProviderError::NotFound) => Err((StatusCode::NOT_FOUND, "Media not found".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn streams_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StreamsQuery>,
    client_headers: HeaderMap,
) -> Result<Json<Vec<WebRelease>>, (StatusCode, String)> {
    let provider = parse_provider(query.provider.as_deref());
    let season = query.season.unwrap_or(0);
    let episode = query.episode.unwrap_or(0);
    let clean_id = clean_id(&id);

    let raw_releases: Vec<Release> = match provider {
        ProviderKind::MovieBox => state
            .service
            .client
            .episode_streams(clean_id, season, episode)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        ProviderKind::FourKHdHub => {
            let fourk = state.service.fourk_client.as_ref().ok_or_else(|| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "4KHDHub not available".to_string(),
                )
            })?;
            fourk
                .episode_streams(&id, season, episode)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        ProviderKind::BdixCircleFtp => state
            .service
            .circleftp_client
            .episode_streams(&id, season, episode)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        ProviderKind::BdixDhakaFlix => state
            .service
            .dhakaflix_client
            .episode_streams(&id, season, episode)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        ProviderKind::Addons => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Addons streams not directly resolvable".to_string(),
            ));
        }
    };

    let base_url = state.base_url(Some(&client_headers));
    let mut web_releases = Vec::new();
    let mut tickets = state.tickets.write().await;

    for rel in raw_releases {
        let mut web_mirrors = Vec::new();
        for mirror in rel.mirrors {
            let ticket_id = format!("{:x}", rand::random::<u128>());
            tickets.insert(
                ticket_id.clone(),
                StreamTicket {
                    url: mirror.resolver_url.clone(),
                    headers: mirror.headers.clone(),
                    created_at: Instant::now(),
                },
            );

            let is_dash =
                mirror.resolver_url.ends_with(".mpd") || mirror.resolver_url.contains("/dash/");
            let proxy_url = if is_dash {
                format!("{base_url}/api/v1/stream/proxy/{ticket_id}/manifest.mpd")
            } else {
                format!("{base_url}/api/v1/stream/proxy/{ticket_id}")
            };

            web_mirrors.push(WebSourceMirror {
                label: mirror.label,
                resolver_url: mirror.resolver_url,
                proxy_url,
                direct_file: mirror.direct_file,
            });
        }

        web_releases.push(WebRelease {
            provider: rel.provider,
            filename: rel.filename,
            quality: rel.quality,
            codec: rel.codec,
            language: rel.language,
            size_bytes: rel.size_bytes,
            season: rel.season,
            episode: rel.episode,
            mirrors: web_mirrors,
            resource_id: rel.resource_id,
        });
    }

    Ok(Json(web_releases))
}

async fn stream_proxy_core(
    state: AppState,
    ticket_id: String,
    file: Option<String>,
    client_headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let ticket = {
        let tickets = state.tickets.read().await;
        tickets.get(&ticket_id).cloned().ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Stream ticket invalid or expired".to_string(),
            )
        })?
    };

    if ticket.created_at.elapsed() > Duration::from_secs(4 * 3600) {
        return Err((StatusCode::GONE, "Stream ticket expired".to_string()));
    }

    let is_dash_manifest_requested = match file.as_deref() {
        None => ticket.url.ends_with(".mpd") || ticket.url.contains("/dash/"),
        Some(f) => {
            let f_clean = f.trim_start_matches('/');
            f_clean == "manifest.mpd" || f_clean == "index.mpd" || f_clean.ends_with(".mpd")
        }
    };

    if is_dash_manifest_requested {
        let mut upstream_req = state.http_client.get(&ticket.url);
        for (name, val) in &ticket.headers {
            if let (Ok(hname), Ok(hval)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(val),
            ) {
                upstream_req = upstream_req.header(hname, hval);
            }
        }

        let upstream_res = upstream_req.send().await.map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Upstream manifest request failed: {e}"),
            )
        })?;

        let manifest_bytes = upstream_res
            .bytes()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let manifest_str = String::from_utf8_lossy(&manifest_bytes);

        // Inject <BaseURL> so DASH player requests all segments through our proxy
        let base_url = state.base_url(Some(&client_headers));
        let base_url_tag =
            format!("<BaseURL>{base_url}/api/v1/stream/proxy/{ticket_id}/</BaseURL>");

        let mut rewritten = manifest_str.into_owned();

        if let Some(b_start) = rewritten.find("<BaseURL>") {
            if let Some(b_end) = rewritten[b_start..].find("</BaseURL>") {
                let full_end = b_start + b_end + "</BaseURL>".len();
                rewritten.replace_range(b_start..full_end, &base_url_tag);
            }
        } else if let Some(period_pos) = rewritten.find("<Period") {
            if let Some(close_tag) = rewritten[period_pos..].find('>') {
                let insert_at = period_pos + close_tag + 1;
                rewritten.insert_str(insert_at, &base_url_tag);
            } else {
                rewritten = rewritten.replace("</MPD>", &format!("{base_url_tag}</MPD>"));
            }
        } else {
            rewritten = rewritten.replace("</MPD>", &format!("{base_url_tag}</MPD>"));
        }

        // Normalize any bare 'codecs="hev1"' to standard RFC 6381 'codecs="hev1.1.6.L93.90"'
        rewritten = rewritten.replace("codecs=\"hev1\"", "codecs=\"hev1.1.6.L93.90\"");
        // Normalize HEVC FourCC from hev1 to hvc1 so Chromium browsers with hardware HEVC can decode
        rewritten = rewritten.replace("codecs=\"hev1", "codecs=\"hvc1");

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/dash+xml; charset=utf-8")
            .header(header::CONTENT_LENGTH, rewritten.len())
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .header(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                "Content-Range, Content-Length, Accept-Ranges, Content-Type",
            )
            .body(Body::from(rewritten))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    let target_url = if let Some(ref filename) = file {
        let clean_filename = filename.trim_start_matches('/');
        if let Some(base_idx) = ticket.url.rfind('/') {
            format!("{}/{}", &ticket.url[..base_idx], clean_filename)
        } else {
            ticket.url.clone()
        }
    } else {
        ticket.url.clone()
    };

    let mut upstream_req = state.http_client.get(&target_url);

    for (name, val) in &ticket.headers {
        if let (Ok(hname), Ok(hval)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(val),
        ) {
            upstream_req = upstream_req.header(hname, hval);
        }
    }

    if let Some(range) = client_headers.get(header::RANGE) {
        upstream_req = upstream_req.header(header::RANGE, range.clone());
    }

    let upstream_res = upstream_req.send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Upstream segment/media request failed: {e}"),
        )
    })?;

    let upstream_status = upstream_res.status();

    let mut response_builder = Response::builder().status(upstream_status.as_u16());

    for (key, value) in upstream_res.headers() {
        let key_str = key.as_str();
        if key_str.eq_ignore_ascii_case("content-type")
            || key_str.eq_ignore_ascii_case("content-length")
            || key_str.eq_ignore_ascii_case("content-range")
            || key_str.eq_ignore_ascii_case("accept-ranges")
            || key_str.eq_ignore_ascii_case("etag")
            || key_str.eq_ignore_ascii_case("last-modified")
        {
            response_builder = response_builder.header(key.clone(), value.clone());
        }
    }

    response_builder = response_builder
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Range, Content-Length, Accept-Ranges, Content-Type",
        );

    let stream = Body::from_stream(upstream_res.bytes_stream());
    response_builder
        .body(stream)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn stream_proxy_query_handler(
    State(state): State<AppState>,
    Query(query): Query<TicketQuery>,
    client_headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    stream_proxy_core(state, query.ticket, query.file, client_headers).await
}

async fn stream_proxy_ticket_handler(
    State(state): State<AppState>,
    Path(ticket): Path<String>,
    client_headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    stream_proxy_core(state, ticket, None, client_headers).await
}

async fn stream_proxy_file_handler(
    State(state): State<AppState>,
    Path((ticket, file)): Path<(String, String)>,
    client_headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    stream_proxy_core(state, ticket, Some(file), client_headers).await
}

async fn subtitles_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SubtitlesQuery>,
    client_headers: HeaderMap,
) -> Result<Json<Vec<WebSubtitleOption>>, (StatusCode, String)> {
    let clean_id = clean_id(&id);
    let resource_id = query.resource_id.unwrap_or_default();
    let base_url = state.base_url(Some(&client_headers));
    match state
        .service
        .get_ext_captions(clean_id, &resource_id, &[])
        .await
    {
        Ok(captions) => {
            let web_subs = captions
                .into_iter()
                .map(|sub| {
                    let encoded_url = percent_encoding::utf8_percent_encode(
                        &sub.url,
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string();
                    let proxy_url = format!("{base_url}/api/v1/subtitles/proxy?url={encoded_url}");
                    WebSubtitleOption {
                        name: sub.name,
                        url: sub.url,
                        proxy_url,
                    }
                })
                .collect();
            Ok(Json(web_subs))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn subtitles_proxy_handler(
    State(state): State<AppState>,
    Query(query): Query<SubtitleProxyQuery>,
) -> Result<Response, (StatusCode, String)> {
    let res = state
        .http_client
        .get(&query.url)
        .header("User-Agent", "MovieBox-Tui/1.0")
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Subtitle fetch failed: {e}"),
            )
        })?;

    let bytes = res
        .bytes()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&bytes);

    let vtt_content = if text.trim_start().starts_with("WEBVTT") {
        text.to_string()
    } else {
        // Convert SubRip (SRT) format to WebVTT
        let mut converted = String::from("WEBVTT\n\n");
        for line in text.lines() {
            if line.contains("-->") {
                // Replace comma decimal separators in timestamps: 00:01:23,456 -> 00:01:23.456
                let vtt_line = line.replace(',', ".");
                converted.push_str(&vtt_line);
            } else {
                converted.push_str(line);
            }
            converted.push('\n');
        }
        converted
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(vtt_content))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to stdout for Render / container visibility
    let log_level = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var("MOVIEBOX_LOG"))
        .unwrap_or_else(|_| "info".to_string());
    if let Ok(logger) = flexi_logger::Logger::try_with_str(&log_level) {
        let _ = logger
            .log_to_stdout()
            .format(flexi_logger::opt_format)
            .start();
    }

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    let public_url = std::env::var("PUBLIC_SERVER_URL")
        .ok()
        .map(|u| u.trim_end_matches('/').to_string());

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let service = Arc::new(MovieBoxService::new());
    let tickets: Arc<RwLock<HashMap<String, StreamTicket>>> = Arc::new(RwLock::new(HashMap::new()));

    // Periodic ticket cleanup task
    let clean_tickets = tickets.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1800));
        loop {
            interval.tick().await;
            let mut lock = clean_tickets.write().await;
            lock.retain(|_, t| t.created_at.elapsed() < Duration::from_secs(4 * 3600));
        }
    });

    let state = AppState {
        service,
        tickets,
        http_client,
        server_port: port,
        public_url: public_url.clone(),
    };

    // Configure CORS: Support CORS_ORIGIN for production security
    let cors = match std::env::var("CORS_ORIGIN").ok().as_deref() {
        Some(origins_str) if !origins_str.trim().is_empty() && origins_str.trim() != "*" => {
            let origins: Vec<HeaderValue> = origins_str
                .split(',')
                .filter_map(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        trimmed.parse::<HeaderValue>().ok()
                    }
                })
                .collect();

            if !origins.is_empty() {
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_credentials(true)
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::PUT,
                        axum::http::Method::DELETE,
                        axum::http::Method::OPTIONS,
                        axum::http::Method::HEAD,
                    ])
                    .allow_headers([
                        header::AUTHORIZATION,
                        header::ACCEPT,
                        header::CONTENT_TYPE,
                        header::ORIGIN,
                        header::RANGE,
                        HeaderName::from_static("x-admin-role"),
                    ])
                    .expose_headers([
                        header::CONTENT_RANGE,
                        header::CONTENT_LENGTH,
                        header::ACCEPT_RANGES,
                        header::CONTENT_TYPE,
                    ])
            } else {
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            }
        }
        _ => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    };

    let app = Router::new()
        .route("/health", get(simple_health_handler))
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/providers", get(providers_handler))
        .route("/api/v1/suggest", get(suggest_handler))
        .route("/api/v1/search", get(search_handler))
        .route("/api/v1/homepage", get(homepage_handler))
        .route("/api/v1/details/{id}", get(details_handler))
        .route("/api/v1/debug/play_info/{id}", get(debug_play_info_handler))
        .route("/api/v1/streams/{id}", get(streams_handler))
        .route("/api/v1/stream/proxy", get(stream_proxy_query_handler))
        .route(
            "/api/v1/stream/proxy/{ticket}",
            get(stream_proxy_ticket_handler),
        )
        .route(
            "/api/v1/stream/proxy/{ticket}/{*file}",
            get(stream_proxy_file_handler),
        )
        .route("/api/v1/subtitles/{id}", get(subtitles_handler))
        .route("/api/v1/subtitles/proxy", get(subtitles_proxy_handler))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("============================================================");
    println!(" CineVault MovieBox-TUI HTTP Gateway Server");
    println!(" Bound to: {}", addr);
    if let Some(ref u) = public_url {
        println!(" Public Base URL: {}", u);
    }
    println!(" Listening on 0.0.0.0:{}", port);
    println!("============================================================");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
