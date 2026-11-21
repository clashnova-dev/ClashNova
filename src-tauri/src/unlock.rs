use crate::{connection::Phase, Core};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::State;

/// One service at a time stays inside this budget. Two-step checks split it as 5s + 3s.
const ITEM_TIMEOUT: Duration = Duration::from_secs(8);
const FIRST_STEP: Duration = Duration::from_secs(5);
const SECOND_STEP: Duration = Duration::from_secs(3);
const CONCURRENCY: usize = 4;
const CACHE_TTL: Duration = Duration::from_secs(600);
const CACHE_MAX: usize = 200;
const BODY_LIMIT: usize = 512 * 1024;
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
/// Public Disney+ web client id (the site's own "disney&browser&1.0.0" token), not a user session.
const DISNEY_TOKEN: &str = "ZGlzbmV5JmJyb3dzZXImMS4wLjA.Cu56AgSfBTDag5NiRA81oLHkDZfu5L3CKadnefEAY84";
const DISNEY_BODY: &str = r#"{"deviceFamily":"browser","applicationRuntime":"chrome","deviceProfile":"windows","attributes":{}}"#;

const SERVICES: &[&str] = &[
    "chatgpt", "claude", "gemini", "copilot", "perplexity", "metaai",
    "netflix", "disney", "youtube", "prime", "max", "paramount", "peacock", "hulu", "dazn", "abema", "bahamut",
    "bilibili", "bbc", "iqiyi", "kktv", "viu", "mytvsuper", "spotify", "tiktok",
    "apple", "wikipedia", "reddit",
];
const DAZN_BODY: &str = r#"{"Version":"2","LandingPageKey":"generic","Languages":"en","Platform":"web","Manufacturer":"","PromoCode":"","PlatformAttributes":{}}"#;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockItem {
    service_id: String,
    node_id: String,
    status: String,
    region: String,
    latency_ms: u64,
    message: String,
    error_code: String,
    tested_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockReport {
    node_id: String,
    cancelled: bool,
    items: Vec<UnlockItem>,
}

struct Judgement {
    status: &'static str,
    region: String,
    message: &'static str,
    error_code: String,
}

struct Fetched {
    status: u16,
    url: String,
    body: String,
    headers: String,
    fail: &'static str,
}

struct Entry {
    item: UnlockItem,
    at: Instant,
}

struct Gate {
    stop: Arc<AtomicBool>,
}

fn gate() -> &'static Mutex<Gate> {
    static GATE: OnceLock<Mutex<Gate>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(Gate { stop: Arc::new(AtomicBool::new(false)) }))
}

fn cache() -> &'static Mutex<HashMap<(String, String), Entry>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), Entry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn begin_run() -> Arc<AtomicBool> {
    let mut gate = gate().lock().unwrap_or_else(|err| err.into_inner());
    gate.stop.store(true, Ordering::SeqCst);
    let stop = Arc::new(AtomicBool::new(false));
    gate.stop = stop.clone();
    stop
}

/// The local proxy URL. There is no direct-client path: a missing port refuses the check.
pub(crate) fn proxy_endpoint(port: u16) -> Result<String, &'static str> {
    if port == 0 {
        return Err("unlock.needConnection");
    }
    Ok(format!("http://127.0.0.1:{port}"))
}

fn proxied(port: u16) -> Result<reqwest::Client, String> {
    let proxy = reqwest::Proxy::all(proxy_endpoint(port).map_err(str::to_string)?)
        .map_err(|_| "unlock.proxyUnavailable")?;
    reqwest::Client::builder()
        .no_proxy()
        .proxy(proxy)
        .redirect(reqwest::redirect::Policy::limited(4))
        .timeout(ITEM_TIMEOUT)
        .build()
        .map_err(|_| "unlock.proxyUnavailable".to_string())
}

/// Two- or three-letter region token. A longer word such as "title" or "INTL" is rejected.
fn alpha2(value: &str) -> String {
    let chars: Vec<char> = value.chars().take(4).collect();
    let letters = chars.iter().take_while(|ch| ch.is_ascii_alphabetic()).count();
    if !(2..=3).contains(&letters) || chars.get(letters).is_some_and(|ch| ch.is_ascii_alphanumeric()) {
        return String::new();
    }
    chars[..letters].iter().collect::<String>().to_ascii_uppercase()
}

fn code(value: &str) -> String {
    value.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-').take(32).collect()
}

fn clip(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn judge(status: &'static str, region: &str, error_code: &str) -> Judgement {
    let (status, message) = match status {
        "available" => ("available", "unlock.available"),
        "partial" => ("partial", "unlock.partial"),
        "region_blocked" => ("region_blocked", "unlock.regionBlocked"),
        "unreachable" => ("unreachable", "unlock.unreachable"),
        _ => ("unknown", "unlock.unknown"),
    };
    Judgement { status, region: alpha2(region), message, error_code: code(error_code) }
}

fn json_string(body: &str, key: &str) -> String {
    json_token(body, key, 16)
}

fn json_token(body: &str, key: &str, max: usize) -> String {
    let pattern = format!("\"{key}\"");
    let Some(pos) = body.find(&pattern) else { return String::new() };
    let Some(rest) = body[pos + pattern.len()..].trim_start().strip_prefix(':') else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix('"') else { return String::new() };
    rest.split('"').next().unwrap_or("").chars().take(max).collect()
}

fn json_bool(body: &str, key: &str) -> Option<bool> {
    let pattern = format!("\"{key}\"");
    let pos = body.find(&pattern)?;
    let rest = body[pos + pattern.len()..].trim_start().strip_prefix(':')?.trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_int(body: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{key}\"");
    let pos = body.find(&pattern)?;
    let rest = body[pos + pattern.len()..].trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find(|ch: char| ch != '-' && !ch.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn netflix_region(body: &str) -> String {
    let mut rest = body;
    while let Some(pos) = rest.find("\"countryName\"") {
        let start = pos.saturating_sub(48);
        let window = &rest[start..pos];
        if let Some(marker) = window.rfind("\"id\":\"") {
            let region = alpha2(&window[marker + 6..]);
            if !region.is_empty() {
                return region;
            }
        }
        rest = &rest[pos + 13..];
    }
    String::new()
}

fn spotify_region(url: &str) -> String {
    let Some(rest) = url.split("spotify.com/").nth(1) else { return String::new() };
    alpha2(rest.split('/').next().unwrap_or(""))
}

fn challenge(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("just a moment") || lower.contains("cf-browser-verification") || lower.contains("cf-challenge")
}

fn has_any(body: &str, markers: &[&str]) -> bool {
    let lower = body.to_ascii_lowercase();
    markers.iter().any(|marker| lower.contains(&marker.to_ascii_lowercase()))
}

fn judge_page(page: &Fetched, ok: &[&str], blocked: &[&str]) -> Judgement {
    if page.fail == "timeout" || page.fail == "connect" {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    if page.status == 451 || has_any(&page.body, blocked) {
        return judge("region_blocked", "", "region");
    }
    if page.status == 403 || page.status >= 500 || page.status == 0 {
        return judge("unreachable", "", &format!("http_{}", page.status));
    }
    if (200..400).contains(&page.status) && has_any(&page.body, ok) {
        return judge("available", "", "");
    }
    judge("unknown", "", "page")
}

fn judge_gemini(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    let url = page.url.to_ascii_lowercase();
    if url.contains("unsupported") || url.contains("google.cn") || page.body.contains("www.google.cn") {
        let region = if url.contains("google.cn") || page.body.contains("www.google.cn") { "CN" } else { "" };
        return judge("region_blocked", region, "region");
    }
    if page.body.contains("45631641,null,true") {
        return judge("available", &after_marker(&page.body, ",2,1,200,\""), "");
    }
    if (200..400).contains(&page.status) && !page.body.is_empty() {
        return judge("region_blocked", "", "region");
    }
    judge("unknown", "", "page")
}

fn judge_chatgpt(web: &Fetched, app: &Fetched) -> Judgement {
    if failed(web) && failed(app) {
        return judge("unreachable", "", web.fail);
    }
    if failed(web) || failed(app) {
        return judge("unknown", "", "partial-response");
    }
    if challenge(&web.body) || challenge(&app.body) {
        return judge("unknown", "", "challenge");
    }
    let web_blocked = web.body.to_ascii_lowercase().contains("unsupported_country");
    let app_blocked = app.body.to_ascii_lowercase().contains("vpn");
    match (web_blocked, app_blocked) {
        (true, true) => judge("region_blocked", "", "region"),
        (false, true) => judge("partial", "", "web-only"),
        (true, false) => judge("partial", "", "app-only"),
        (false, false) => judge("available", "", ""),
    }
}

fn judge_claude(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    let url = page.url.to_ascii_lowercase();
    if url.contains("app-unavailable-in-region") {
        return judge("region_blocked", "", "region");
    }
    if url == "https://claude.ai" || url == "https://claude.ai/" || url.starts_with("https://claude.ai/") {
        return judge("available", "", "");
    }
    judge("unknown", "", "page")
}

fn judge_youtube(page: &Fetched) -> Judgement {
    if page.fail == "timeout" || page.fail == "connect" {
        return judge("unreachable", "", page.fail);
    }
    let region = alpha2(&json_string(&page.body, "INNERTUBE_CONTEXT_GL"));
    if page.body.contains("www.google.cn") {
        return judge("region_blocked", "CN", "region");
    }
    if has_any(&page.body, &["Premium is not available in your country"]) {
        return judge("region_blocked", &region, "region");
    }
    if has_any(&page.body, &["ad-free"]) {
        return judge("available", &region, "");
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    judge("unknown", "", "page")
}

fn judge_netflix(first: &Fetched, second: &Fetched) -> Judgement {
    let down = |page: &Fetched| page.fail == "timeout" || page.fail == "connect";
    if down(first) && down(second) {
        return judge("unreachable", "", first.fail);
    }
    if down(first) || down(second) {
        return judge("unknown", "", "partial-response");
    }
    let region = {
        let found = netflix_region(&first.body);
        if found.is_empty() { netflix_region(&second.body) } else { found }
    };
    let blocked = |page: &Fetched| page.body.contains("Oh no!");
    if blocked(first) && blocked(second) {
        return judge("partial", &region, "catalog");
    }
    let offered = |page: &Fetched| !blocked(page) && (page.body.contains("countryName") || page.body.contains("netflix.watch"));
    if offered(first) || offered(second) {
        return judge("available", &region, "");
    }
    if (first.status == 403 || first.status == 451) && (second.status == 403 || second.status == 451) {
        return judge("region_blocked", &region, "region");
    }
    judge("unknown", "", "page")
}

fn judge_disney(device: &Fetched, home: Option<&Fetched>) -> Judgement {
    if device.fail == "timeout" || device.fail == "connect" {
        return judge("unreachable", "", device.fail);
    }
    let lower = device.body.to_ascii_lowercase();
    if device.status == 403 || lower.contains("403 error") || lower.contains("forbidden-location") {
        return judge("region_blocked", "", "region");
    }
    let home = match home {
        Some(page) if page.fail != "timeout" && page.fail != "connect" => page,
        _ => return judge("unknown", "", "page"),
    };
    let url = home.url.to_ascii_lowercase();
    let region = alpha2(&json_string(&home.body, "countryCode"));
    if url.contains("unavailable") || url.contains("preview") {
        return judge("region_blocked", &region, "region");
    }
    match json_bool(&home.body, "inSupportedLocation") {
        Some(true) if !region.is_empty() => return judge("available", &region, ""),
        Some(false) if !region.is_empty() => return judge("partial", &region, "catalog"),
        _ => {}
    }
    if lower.contains("assertion") {
        return judge("unknown", "", "page");
    }
    judge("unknown", "", "page")
}

fn judge_tiktok(page: &Fetched) -> Judgement {
    if page.fail == "timeout" || page.fail == "connect" {
        return judge("unreachable", "", page.fail);
    }
    if has_any(&page.body, &["not available in your country", "not available in your region"]) {
        return judge("region_blocked", "", "region");
    }
    let region = alpha2(&json_string(&page.body, "region"));
    if (200..400).contains(&page.status) && !region.is_empty() && !challenge(&page.body) {
        return judge("available", &region, "");
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    judge("unknown", "", "page")
}

fn judge_spotify(page: &Fetched) -> Judgement {
    if page.fail == "timeout" || page.fail == "connect" {
        return judge("unreachable", "", page.fail);
    }
    if has_any(&page.body, &["not available in your country"]) {
        return judge("region_blocked", "", "region");
    }
    let region = spotify_region(&page.url);
    if (200..400).contains(&page.status) && !region.is_empty() && !challenge(&page.body) {
        return judge("available", &region, "");
    }
    judge("unknown", "", "page")
}

fn judge_bilibili(page: &Fetched) -> Judgement {
    if page.fail == "timeout" || page.fail == "connect" {
        return judge("unreachable", "", page.fail);
    }
    match json_int(&page.body, "code") {
        Some(0) => judge("available", "", ""),
        Some(-10403) => judge("region_blocked", "", "region"),
        Some(other) => judge("unknown", "", &other.to_string()),
        None => judge("unknown", "", "page"),
    }
}

fn judge_prime(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    if page.body.to_ascii_lowercase().contains("isservicerestricted") {
        return judge("region_blocked", "", "region");
    }
    let region = alpha2(&json_string(&page.body, "currentTerritory"));
    if !region.is_empty() {
        return judge("available", &region, "");
    }
    judge("unknown", "", "page")
}

fn judge_max(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    if has_any(&page.body, &["not available in your region", "isn't available in your region"]) {
        return judge("region_blocked", "", "region");
    }
    let mut region = after_marker(&page.body, "countryCode=");
    if region.is_empty() {
        region = alpha2(&json_string(&page.body, "countryCode"));
    }
    if !region.is_empty() {
        return judge("available", &region, "");
    }
    judge("unknown", "", "page")
}

fn judge_paramount(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    let segment = slash_field(&page.url, 3).to_ascii_uppercase();
    if segment == "INTL" {
        return judge("region_blocked", "", "region");
    }
    if page.status != 200 {
        return judge("unknown", "", "page");
    }
    let region = if segment.is_empty() { "US".to_string() } else { alpha2(&segment) };
    if region.is_empty() {
        return judge("unknown", "", "page");
    }
    judge("available", &region, "")
}

fn judge_peacock(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    if page.url.to_ascii_lowercase().contains("unavailable") {
        return judge("region_blocked", "", "region");
    }
    if page.status == 200 && page.url.to_ascii_lowercase().contains("peacocktv.com") {
        return judge("available", "", "");
    }
    judge("unknown", "", "page")
}

fn judge_hulu(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    let url = page.url.to_ascii_lowercase();
    if url.contains("restrict") || page.status == 403 {
        return judge("region_blocked", "", "region");
    }
    if page.status == 200 && url.contains("hulu.jp") {
        return judge("available", "JP", "");
    }
    judge("unknown", "", "page")
}

fn judge_dazn(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if page.body.to_ascii_lowercase().contains("security policy has been breached") {
        return judge("region_blocked", "", "region");
    }
    let region = alpha2(&json_string(&page.body, "GeolocatedCountry"));
    match json_bool(&page.body, "isAllowed") {
        Some(true) => judge("available", &region, ""),
        Some(false) => judge("region_blocked", &region, "region"),
        None => judge("unknown", "", "page"),
    }
}

fn judge_abema(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    let region = alpha2(&json_string(&page.body, "isoCountryCode"));
    if region == "JP" {
        return judge("available", "JP", "");
    }
    if !region.is_empty() {
        return judge("partial", &region, "catalog");
    }
    if !page.body.is_empty() {
        return judge("region_blocked", "", "region");
    }
    judge("unknown", "", "page")
}

fn judge_bahamut(device: &Fetched, token: Option<&Fetched>, home: Option<&Fetched>) -> Judgement {
    if failed(device) {
        return judge("unreachable", "", device.fail);
    }
    let Some(token) = token else { return judge("unknown", "", "page") };
    if failed(token) {
        return judge("unknown", "", "partial-response");
    }
    if !token.body.contains("animeSn") {
        return judge("region_blocked", "", "region");
    }
    let Some(home) = home else { return judge("unknown", "", "page") };
    if failed(home) {
        return judge("unknown", "", "partial-response");
    }
    let region = after_marker(&home.body, "data-geo=\"");
    if region.is_empty() {
        return judge("unknown", "", "page");
    }
    judge("available", &region, "")
}

fn judge_bbc(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    let lower = page.body.to_ascii_lowercase();
    if lower.contains("geolocation") {
        return judge("region_blocked", "", "region");
    }
    if lower.contains("vs-hls-push-uk") {
        return judge("available", "GB", "");
    }
    judge("unknown", "", "page")
}

fn judge_iqiyi(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    let mut raw = cookie_token(&page.headers, "mod");
    if raw.is_empty() {
        raw = cookie_token(&page.body, "mod");
    }
    if raw.is_empty() {
        return judge("unknown", "", "page");
    }
    let upper = raw.to_ascii_uppercase();
    let region = if upper == "NTW" { "TW".to_string() } else { alpha2(&upper) };
    if region.is_empty() {
        return judge("unknown", "", "page");
    }
    judge("available", &region, "")
}

fn judge_kktv(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    let region = alpha2(&json_string(&page.body, "country"));
    if region == "TW" {
        return judge("available", "TW", "");
    }
    if !region.is_empty() {
        return judge("region_blocked", &region, "region");
    }
    judge("unknown", "", "page")
}

fn judge_viu(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    let segment = slash_field(&page.url, 4).to_ascii_uppercase();
    if segment == "NO-SERVICE" {
        return judge("region_blocked", "", "region");
    }
    if segment.is_empty() {
        return judge("unknown", "", "page");
    }
    if page.status == 200 {
        let region = alpha2(&segment);
        if !region.is_empty() {
            return judge("available", &region, "");
        }
    }
    judge("unknown", "", "page")
}

fn judge_mytvsuper(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    let region = alpha2(&json_string(&page.body, "country_code"));
    if region == "HK" {
        return judge("available", "HK", "");
    }
    if !region.is_empty() {
        return judge("region_blocked", &region, "region");
    }
    judge("unknown", "", "page")
}

fn judge_apple(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    let region = alpha2(page.body.trim());
    if !region.is_empty() && page.body.trim().chars().all(|ch| ch.is_ascii_alphabetic()) {
        return judge("available", &region, "");
    }
    judge("unknown", "", "page")
}

fn judge_wikipedia(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if page.body.to_ascii_lowercase().contains("banned") {
        return judge("region_blocked", "", "region");
    }
    if page.status == 200 && page.body.to_ascii_lowercase().contains("wikipedia") {
        return judge("available", "", "");
    }
    judge("unknown", "", "page")
}

fn judge_reddit(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    if page.status == 403 {
        return judge("region_blocked", "", "region");
    }
    if page.status == 200 && page.url.to_ascii_lowercase().contains("reddit.com") {
        return judge("available", "", "");
    }
    judge("unknown", "", "page")
}

fn judge_meta(page: &Fetched) -> Judgement {
    if failed(page) {
        return judge("unreachable", "", page.fail);
    }
    if challenge(&page.body) {
        return judge("unknown", "", "challenge");
    }
    if page.body.contains("AbraGeoBlockedErrorRoot") {
        return judge("region_blocked", "", "region");
    }
    if page.body.contains("KadabraRootContainer") {
        let raw = json_string(&page.body, "code");
        let region = alpha2(raw.split('_').nth(1).unwrap_or(""));
        return judge("available", &region, "");
    }
    judge("unknown", "", "page")
}

fn trace_region(body: &str) -> String {
    body.lines().find_map(|line| line.strip_prefix("loc=")).map(alpha2).unwrap_or_default()
}

fn apply_exit_region(item: &mut UnlockItem, exit_region: &str) {
    if item.region.is_empty() && item.status != "unreachable" {
        item.region = alpha2(exit_region);
    }
}

fn cache_key(port: u16, node: &str, service: &str) -> (String, String) {
    (format!("{port}:{node}"), service.to_string())
}

fn cache_get(map: &HashMap<(String, String), Entry>, key: &(String, String), now: Instant, refresh: bool) -> Option<UnlockItem> {
    if refresh {
        return None;
    }
    let entry = map.get(key)?;
    if now.saturating_duration_since(entry.at) > CACHE_TTL {
        return None;
    }
    Some(entry.item.clone())
}

fn cache_put(map: &mut HashMap<(String, String), Entry>, key: (String, String), item: UnlockItem, now: Instant) {
    if item.status == "unreachable" {
        return;
    }
    map.insert(key, Entry { item, at: now });
    if map.len() <= CACHE_MAX {
        return;
    }
    if let Some(oldest) = map.iter().min_by_key(|(_, entry)| entry.at).map(|(key, _)| key.clone()) {
        map.remove(&oldest);
    }
}

fn known_services(requested: &[String]) -> Vec<String> {
    let wanted: Vec<String> = if requested.is_empty() {
        SERVICES.iter().map(|id| (*id).to_string()).collect()
    } else {
        requested.iter().filter(|id| SERVICES.contains(&id.as_str())).cloned().collect()
    };
    let mut seen = std::collections::HashSet::new();
    wanted.into_iter().filter(|id| seen.insert(id.clone())).collect()
}

async fn read_body(mut response: reqwest::Response) -> Result<String, ()> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        let room = BODY_LIMIT.saturating_sub(bytes.len());
        if room == 0 {
            break;
        }
        let take = chunk.len().min(room);
        bytes.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn response_headers(response: &reqwest::Response) -> String {
    let mut out = String::new();
    for name in ["set-cookie", "location"] {
        for value in response.headers().get_all(name) {
            let Ok(text) = value.to_str() else { continue };
            if out.len() >= 4096 {
                return out;
            }
            let room = 4096 - out.len();
            out.push_str(&text.chars().take(room).collect::<String>());
            out.push('\n');
        }
    }
    out
}

async fn fetch(client: &reqwest::Client, url: &str, json: Option<&str>, auth: Option<&str>, timeout: Duration) -> Fetched {
    let mut request = if json.is_some() { client.post(url) } else { client.get(url) };
    request = request
        .timeout(timeout)
        .header("user-agent", UA)
        .header("accept-language", "en-US,en;q=0.9")
        .header("accept", "text/html,application/json;q=0.9,*/*;q=0.8");
    if url.contains("indazn.com") {
        request = request.header("origin", "https://www.dazn.com").header("referer", "https://www.dazn.com/");
    }
    if let Some(token) = auth {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(raw) = json {
        request = request.header("content-type", "application/json").body(raw.to_string());
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let url = response.url().to_string();
            let headers = response_headers(&response);
            let body = read_body(response).await.unwrap_or_default();
            Fetched { status, url, body, headers, fail: "" }
        }
        Err(err) => Fetched {
            status: 0,
            url: url.to_string(),
            body: String::new(),
            headers: String::new(),
            fail: if err.is_timeout() { "timeout" } else { "connect" },
        },
    }
}

fn failed(page: &Fetched) -> bool {
    page.fail == "timeout" || page.fail == "connect"
}

fn slash_field(url: &str, index: usize) -> String {
    url.split('?').next().unwrap_or(url).split('/').nth(index).unwrap_or("").to_string()
}

fn after_marker(body: &str, marker: &str) -> String {
    body.find(marker).map(|pos| alpha2(&body[pos + marker.len()..])).unwrap_or_default()
}

fn cookie_token(headers: &str, key: &str) -> String {
    let pattern = format!("{key}=");
    let Some(pos) = headers.to_ascii_lowercase().find(&pattern.to_ascii_lowercase()) else { return String::new() };
    headers[pos + pattern.len()..]
        .split(|ch: char| ch == ';' || ch == '\n' || ch == ' ' || ch == ',')
        .next()
        .unwrap_or("")
        .chars()
        .take(8)
        .collect()
}

fn finish(node_id: &str, service_id: &str, judged: Judgement, started: Instant) -> UnlockItem {
    UnlockItem {
        service_id: service_id.to_string(),
        node_id: clip(node_id, 128),
        status: judged.status.to_string(),
        region: judged.region,
        latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        message: judged.message.to_string(),
        error_code: judged.error_code,
        tested_at: chrono::Utc::now().timestamp_millis(),
    }
}

async fn probe(client: &reqwest::Client, node_id: &str, service: &str) -> UnlockItem {
    let started = Instant::now();
    let judged = match service {
        "chatgpt" => {
            let web = fetch(client, "https://api.openai.com/compliance/cookie_requirements", None, None, FIRST_STEP).await;
            let app = fetch(client, "https://ios.chat.openai.com/", None, None, SECOND_STEP).await;
            judge_chatgpt(&web, &app)
        },
        "claude" => judge_claude(&fetch(client, "https://claude.ai/", None, None, ITEM_TIMEOUT).await),
        "gemini" => judge_gemini(&fetch(client, "https://gemini.google.com/", None, None, ITEM_TIMEOUT).await),
        "copilot" => judge_page(
            &fetch(client, "https://copilot.microsoft.com/", None, None, ITEM_TIMEOUT).await,
            &["copilot"],
            &["not available in your region", "isn't available in your region"],
        ),
        "perplexity" => judge_page(
            &fetch(client, "https://www.perplexity.ai/", None, None, ITEM_TIMEOUT).await,
            &["perplexity"],
            &["not available in your country", "not available in your region"],
        ),
        "metaai" => judge_meta(&fetch(client, "https://www.meta.ai/", None, None, ITEM_TIMEOUT).await),
        "youtube" => judge_youtube(&fetch(client, "https://www.youtube.com/premium", None, None, ITEM_TIMEOUT).await),
        "tiktok" => judge_tiktok(&fetch(client, "https://www.tiktok.com/", None, None, ITEM_TIMEOUT).await),
        "spotify" => judge_spotify(&fetch(client, "https://www.spotify.com/", None, None, ITEM_TIMEOUT).await),
        "netflix" => {
            let first = fetch(client, "https://www.netflix.com/title/81280792", None, None, FIRST_STEP).await;
            let second = if first.fail == "timeout" || first.fail == "connect" {
                first.fail
            } else {
                ""
            };
            let second = if second.is_empty() {
                fetch(client, "https://www.netflix.com/title/70143836", None, None, SECOND_STEP).await
            } else {
                Fetched { status: 0, url: String::new(), body: String::new(), headers: String::new(), fail: second }
            };
            judge_netflix(&first, &second)
        }
        "disney" => {
            let device = fetch(client, "https://disney.api.edge.bamgrid.com/devices", Some(DISNEY_BODY), Some(DISNEY_TOKEN), FIRST_STEP).await;
            let home = if device.fail.is_empty() && device.status != 403 && !device.body.to_ascii_lowercase().contains("forbidden-location") {
                Some(fetch(client, "https://www.disneyplus.com/", None, None, SECOND_STEP).await)
            } else {
                None
            };
            judge_disney(&device, home.as_ref())
        }
        "bilibili" => {
            let session = format!("{:032x}", chrono::Utc::now().timestamp_millis() as u128);
            let url = format!("https://api.bilibili.com/pgc/player/web/playurl?avid=18281381&cid=29892777&qn=0&type=&otype=json&ep_id=183799&fourk=1&fnver=0&fnval=16&session={session}&module=bangumi");
            judge_bilibili(&fetch(client, &url, None, None, ITEM_TIMEOUT).await)
        }
        "prime" => judge_prime(&fetch(client, "https://www.primevideo.com/", None, None, ITEM_TIMEOUT).await),
        "max" => judge_max(&fetch(client, "https://www.max.com/", None, None, ITEM_TIMEOUT).await),
        "paramount" => judge_paramount(&fetch(client, "https://www.paramountplus.com/", None, None, ITEM_TIMEOUT).await),
        "peacock" => judge_peacock(&fetch(client, "https://www.peacocktv.com/", None, None, ITEM_TIMEOUT).await),
        "hulu" => judge_hulu(&fetch(client, "https://id.hulu.jp/", None, None, ITEM_TIMEOUT).await),
        "dazn" => judge_dazn(&fetch(client, "https://startup.core.indazn.com/misl/v5/Startup", Some(DAZN_BODY), None, ITEM_TIMEOUT).await),
        "abema" => judge_abema(&fetch(client, "https://api.abema.io/v1/ip/check?device=android", None, None, ITEM_TIMEOUT).await),
        "bahamut" => {
            let device = fetch(client, "https://ani.gamer.com.tw/ajax/getdeviceid.php", None, None, Duration::from_secs(3)).await;
            let id: String = json_token(&device.body, "deviceid", 80).chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(64).collect();
            let (token, home) = if failed(&device) || id.is_empty() {
                (None, None)
            } else {
                let token = fetch(client, &format!("https://ani.gamer.com.tw/ajax/token.php?adID=89422&sn=37783&device={id}"), None, None, Duration::from_secs(3)).await;
                let home = if token.body.contains("animeSn") {
                    Some(fetch(client, "https://ani.gamer.com.tw/", None, None, Duration::from_secs(2)).await)
                } else {
                    None
                };
                (Some(token), home)
            };
            judge_bahamut(&device, token.as_ref(), home.as_ref())
        }
        "bbc" => judge_bbc(&fetch(client, "https://open.live.bbc.co.uk/mediaselector/6/select/version/2.0/mediaset/pc/vpid/bbc_one_london/format/json/jsfunc/JS_callbacks0", None, None, ITEM_TIMEOUT).await),
        "iqiyi" => judge_iqiyi(&fetch(client, "https://www.iq.com/", None, None, ITEM_TIMEOUT).await),
        "kktv" => judge_kktv(&fetch(client, "https://api.kktv.me/v3/ipcheck", None, None, ITEM_TIMEOUT).await),
        "viu" => judge_viu(&fetch(client, "https://www.viu.com/", None, None, ITEM_TIMEOUT).await),
        "mytvsuper" => judge_mytvsuper(&fetch(client, "https://www.mytvsuper.com/api/auth/getSession/self/", None, None, ITEM_TIMEOUT).await),
        "apple" => judge_apple(&fetch(client, "https://gspe1-ssl.ls.apple.com/pep/gcc", None, None, ITEM_TIMEOUT).await),
        "wikipedia" => judge_wikipedia(&fetch(client, "https://en.wikipedia.org/w/index.php?title=Wikipedia:Sandbox&action=edit", None, None, ITEM_TIMEOUT).await),
        "reddit" => judge_reddit(&fetch(client, "https://www.reddit.com/", None, None, ITEM_TIMEOUT).await),
        _ => judge("unknown", "", "service"),
    };
    finish(node_id, service, judged, started)
}

async fn run_chunk(client: &reqwest::Client, node_id: &str, ids: &[String]) -> Vec<UnlockItem> {
    match ids {
        [a] => vec![probe(client, node_id, a).await],
        [a, b] => {
            let (x, y) = tokio::join!(probe(client, node_id, a), probe(client, node_id, b));
            vec![x, y]
        }
        [a, b, c] => {
            let (x, y, z) = tokio::join!(probe(client, node_id, a), probe(client, node_id, b), probe(client, node_id, c));
            vec![x, y, z]
        }
        [a, b, c, d] => {
            let (w, x, y, z) = tokio::join!(
                probe(client, node_id, a),
                probe(client, node_id, b),
                probe(client, node_id, c),
                probe(client, node_id, d)
            );
            vec![w, x, y, z]
        }
        _ => Vec::new(),
    }
}

async fn exit_region(client: &reqwest::Client) -> String {
    let page = fetch(client, "https://www.cloudflare.com/cdn-cgi/trace", None, None, Duration::from_secs(4)).await;
    if !page.fail.is_empty() {
        return String::new();
    }
    trace_region(&page.body)
}

fn remember(port: u16, node_id: &str, item: &UnlockItem) {
    let mut map = cache().lock().unwrap_or_else(|err| err.into_inner());
    cache_put(&mut map, cache_key(port, node_id, &item.service_id), item.clone(), Instant::now());
}

#[tauri::command]
pub fn unlock_cancel() {
    if let Ok(gate) = gate().lock() {
        gate.stop.store(true, Ordering::SeqCst);
    }
}

/// Every request uses the connected local proxy. Nothing here falls back to a direct client.
#[tauri::command]
pub async fn unlock_check(state: State<'_, Arc<Mutex<Core>>>, node_id: String, services: Vec<String>, refresh: bool) -> Result<UnlockReport, String> {
    let port = {
        let core = state.inner().try_lock().map_err(|_| "native.connectionStateBusy")?;
        if core.phase != Phase::Connected || core.network.proxy_port == 0 {
            return Err("unlock.needConnection".into());
        }
        core.network.proxy_port
    };
    let node_id = clip(&node_id, 128);
    let client = proxied(port)?;
    let stop = begin_run();
    let ids = known_services(&services);
    let mut fresh = Vec::new();
    let mut items = Vec::new();
    {
        let map = cache().lock().unwrap_or_else(|err| err.into_inner());
        let now = Instant::now();
        for id in &ids {
            match cache_get(&map, &cache_key(port, &node_id, id), now, refresh) {
                Some(hit) => items.push(hit),
                None => fresh.push(id.clone()),
            }
        }
    }
    let mut cancelled = stop.load(Ordering::SeqCst);
    let mut exit = String::new();
    let mut chunks = fresh.chunks(CONCURRENCY);
    if !cancelled {
        if let Some(first) = chunks.next() {
            let client_for_region = client.clone();
            let (batch, region) = tokio::join!(run_chunk(&client, &node_id, first), exit_region(&client_for_region));
            exit = region;
            for mut hit in batch {
                apply_exit_region(&mut hit, &exit);
                remember(port, &node_id, &hit);
                items.push(hit);
            }
        }
    }
    for chunk in chunks {
        if stop.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        let batch = run_chunk(&client, &node_id, chunk).await;
        for mut hit in batch {
            apply_exit_region(&mut hit, &exit);
            remember(port, &node_id, &hit);
            items.push(hit);
        }
    }
    if stop.load(Ordering::SeqCst) {
        cancelled = true;
    }
    let mut ordered = Vec::new();
    for id in &ids {
        if let Some(hit) = items.iter().find(|item| item.service_id == *id) {
            ordered.push(hit.clone());
        }
    }
    Ok(UnlockReport { node_id, cancelled, items: ordered })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(status: u16, url: &str, body: &str) -> Fetched {
        Fetched { status, url: url.into(), body: body.into(), headers: String::new(), fail: "" }
    }
    fn headed(status: u16, url: &str, body: &str, headers: &str) -> Fetched {
        Fetched { status, url: url.into(), body: body.into(), headers: headers.into(), fail: "" }
    }
    fn down(kind: &'static str) -> Fetched {
        Fetched { status: 0, url: String::new(), body: String::new(), headers: String::new(), fail: kind }
    }

    #[test]
    fn the_proxy_endpoint_is_local_and_refuses_a_missing_port() {
        assert_eq!(proxy_endpoint(0), Err("unlock.needConnection"));
        assert_eq!(proxy_endpoint(2080).unwrap(), "http://127.0.0.1:2080");
    }

    #[test]
    fn netflix_distinguishes_a_full_catalog_from_a_limited_one() {
        let full = page(200, "https://www.netflix.com/title/81280792", r#"{"id":"SG","countryName":"Singapore"} netflix.watch"#);
        let missing = page(200, "https://www.netflix.com/title/70143836", "Oh no! This title is not available");
        let open = judge_netflix(&full, &missing);
        assert_eq!(open.status, "available");
        assert_eq!(open.region, "SG");
        let limited = judge_netflix(&missing, &missing);
        assert_eq!(limited.status, "partial");
        assert_eq!(judge_netflix(&down("timeout"), &down("connect")).status, "unreachable");
        assert_eq!(judge_netflix(&page(200, "", "login wall"), &page(200, "", "login wall")).status, "unknown");
    }

    #[test]
    fn youtube_requires_an_explicit_premium_or_region_signal() {
        let yes = judge_youtube(&page(200, "https://www.youtube.com/premium", r#"ad-free "INNERTUBE_CONTEXT_GL":"US""#));
        assert_eq!(yes.status, "available");
        assert_eq!(yes.region, "US");
        assert_eq!(judge_youtube(&page(200, "", "Premium is not available in your country")).status, "region_blocked");
        assert_eq!(judge_youtube(&page(200, "https://www.google.cn", "www.google.cn")).region, "CN");
        assert_eq!(judge_youtube(&down("timeout")).status, "unreachable");
        assert_eq!(judge_youtube(&page(200, "", "Just a moment")).status, "unknown");
    }

    #[test]
    fn bilibili_uses_the_playback_code() {
        assert_eq!(judge_bilibili(&page(200, "", r#"{"code":0}"#)).status, "available");
        assert_eq!(judge_bilibili(&page(200, "", r#"{"code": -10403}"#)).status, "region_blocked");
        assert_eq!(judge_bilibili(&page(200, "", r#"{"code":-400}"#)).status, "unknown");
        assert_eq!(judge_bilibili(&down("connect")).status, "unreachable");
    }

    #[test]
    fn spotify_tiktok_and_gemini_need_a_clear_signal() {
        assert_eq!(judge_spotify(&page(200, "https://www.spotify.com/hk-en/", "spotify")).status, "available");
        assert_eq!(judge_spotify(&page(200, "https://www.spotify.com/hk-en/", "spotify")).region, "HK");
        assert_eq!(judge_spotify(&page(200, "https://www.spotify.com/", "not available in your country")).status, "region_blocked");
        assert_eq!(judge_tiktok(&page(200, "https://www.tiktok.com/", r#"{"region":"JP"}"#)).status, "available");
        assert_eq!(judge_tiktok(&page(200, "", "welcome")).status, "unknown");
        assert_eq!(judge_gemini(&page(302, "https://gemini.google.com/unsupported", "")).status, "region_blocked");
        assert_eq!(judge_page(&page(200, "", "Just a moment chatgpt"), &["chatgpt"], &["unsupported_country"]).status, "unknown");
        assert_eq!(judge_page(&page(200, "", "Welcome to ChatGPT"), &["chatgpt"], &["unsupported_country"]).status, "available");
        assert_eq!(judge_page(&page(403, "", "unsupported_country"), &["chatgpt"], &["unsupported_country"]).status, "region_blocked");
    }

    #[test]
    fn disney_uses_the_location_flag_and_does_not_guess() {
        let device = page(200, "", r#"{"assertion":"abc"}"#);
        let open = page(200, "https://www.disneyplus.com/", r#"{"countryCode":"JP","inSupportedLocation":true}"#);
        let soon = page(200, "https://www.disneyplus.com/", r#"{"countryCode":"IN","inSupportedLocation":false}"#);
        let banned = page(403, "", "403 ERROR");
        assert_eq!(judge_disney(&device, Some(&open)).status, "available");
        assert_eq!(judge_disney(&device, Some(&open)).region, "JP");
        assert_eq!(judge_disney(&device, Some(&soon)).status, "partial");
        assert_eq!(judge_disney(&banned, None).status, "region_blocked");
        assert_eq!(judge_disney(&device, None).status, "unknown");
    }

    #[test]
    fn cache_expires_and_a_refresh_misses() {
        let mut map = HashMap::new();
        let key = cache_key(2080, "node-1", "netflix");
        let item = UnlockItem {
            service_id: "netflix".into(),
            node_id: "node-1".into(),
            status: "available".into(),
            region: "SG".into(),
            latency_ms: 10,
            message: "unlock.available".into(),
            error_code: String::new(),
            tested_at: 1,
        };
        let now = Instant::now();
        cache_put(&mut map, key.clone(), item, now);
        assert_eq!(cache_get(&map, &key, now, false).unwrap().region, "SG");
        assert!(cache_get(&map, &key, now, true).is_none());
        assert!(cache_get(&map, &key, now + CACHE_TTL + Duration::from_secs(1), false).is_none());
        let failed = UnlockItem { status: "unreachable".into(), ..cache_get(&map, &key, now, false).unwrap_or_else(|| UnlockItem {
            service_id: "netflix".into(), node_id: "node-1".into(), status: "unreachable".into(), region: String::new(),
            latency_ms: 0, message: "unlock.unreachable".into(), error_code: String::new(), tested_at: 0,
        }) };
        cache_put(&mut map, ("other".into(), "netflix".into()), failed, now);
        assert!(map.get(&("other".into(), "netflix".into())).is_none());
    }

    #[test]
    fn the_exit_region_fills_only_a_response_that_came_back() {
        let mut item = UnlockItem {
            service_id: "claude".into(), node_id: "n".into(), status: "available".into(), region: String::new(),
            latency_ms: 1, message: "unlock.available".into(), error_code: String::new(), tested_at: 0,
        };
        apply_exit_region(&mut item, "de");
        assert_eq!(item.region, "DE");
        item.status = "unreachable".into();
        item.region.clear();
        apply_exit_region(&mut item, "DE");
        assert!(item.region.is_empty());
        assert_eq!(trace_region("ip=203.0.113.8\nloc=SG\n"), "SG");
        assert!(known_services(&vec!["netflix".into(), "nope".into(), "netflix".into()]) == vec!["netflix".to_string()]);
    }

    #[test]
    fn added_services_need_an_explicit_signal() {
        assert_eq!(SERVICES.len(), 28);
        assert_eq!(alpha2("USA\""), "USA");
        assert_eq!(alpha2("hk-en"), "HK");
        assert!(alpha2("title").is_empty());
        assert!(alpha2("INTL").is_empty());

        let clean = page(200, "https://api.openai.com/compliance/cookie_requirements", "requirements");
        let app = page(200, "https://ios.chat.openai.com/", "chat");
        assert_eq!(judge_chatgpt(&clean, &app).status, "available");
        assert_eq!(judge_chatgpt(&page(200, "", "unsupported_country"), &page(200, "", "VPN")).status, "region_blocked");
        assert_eq!(judge_chatgpt(&page(200, "", "ok"), &page(200, "", "use a VPN")).status, "partial");
        assert_eq!(judge_chatgpt(&page(200, "", "unsupported_country"), &app).status, "partial");
        assert_eq!(judge_chatgpt(&down("timeout"), &down("connect")).status, "unreachable");
        assert_eq!(judge_chatgpt(&down("timeout"), &app).status, "unknown");

        assert_eq!(judge_claude(&page(200, "https://claude.ai/login", "")).status, "available");
        assert_eq!(judge_claude(&page(200, "https://www.anthropic.com/app-unavailable-in-region", "")).status, "region_blocked");
        assert_eq!(judge_claude(&page(200, "https://www.anthropic.com/", "")).status, "unknown");
        let gemini = page(200, "https://gemini.google.com/", r#"45631641,null,true ,2,1,200,"USA""#);
        assert_eq!(judge_gemini(&gemini).status, "available");
        assert_eq!(judge_gemini(&gemini).region, "USA");
        assert_eq!(judge_gemini(&page(200, "https://gemini.google.com/", "Welcome to Gemini")).status, "region_blocked");
        assert_eq!(judge_gemini(&page(200, "https://gemini.google.com/", "Just a moment")).status, "unknown");

        assert_eq!(judge_prime(&page(200, "", r#"{"currentTerritory":"JP"}"#)).region, "JP");
        assert_eq!(judge_prime(&page(200, "", r#"{"isServiceRestricted":true}"#)).status, "region_blocked");
        assert_eq!(judge_prime(&page(200, "", "prime")).status, "unknown");
        assert_eq!(judge_max(&page(200, "https://www.max.com/", "countryCode=SG")).region, "SG");
        assert_eq!(judge_max(&page(200, "", "welcome")).status, "unknown");
        assert_eq!(judge_paramount(&page(200, "https://www.paramountplus.com/", "")).region, "US");
        assert_eq!(judge_paramount(&page(200, "https://www.paramountplus.com/ca/", "")).region, "CA");
        assert_eq!(judge_paramount(&page(200, "https://www.paramountplus.com/intl", "")).status, "region_blocked");
        assert_eq!(judge_peacock(&page(200, "https://www.peacocktv.com/", "")).status, "available");
        assert_eq!(judge_peacock(&page(200, "https://www.peacocktv.com/unavailable", "")).status, "region_blocked");
        assert_eq!(judge_hulu(&page(200, "https://id.hulu.jp/", "")).region, "JP");
        assert_eq!(judge_hulu(&page(200, "https://id.hulu.jp/restrict", "")).status, "region_blocked");
        assert_eq!(judge_dazn(&page(200, "", r#"{"isAllowed":true,"GeolocatedCountry":"DE"}"#)).status, "available");
        assert_eq!(judge_dazn(&page(200, "", "Security policy has been breached")).status, "region_blocked");
        assert_eq!(judge_abema(&page(200, "", r#"{"isoCountryCode":"JP"}"#)).status, "available");
        assert_eq!(judge_abema(&page(200, "", r#"{"isoCountryCode":"TW"}"#)).status, "partial");
        assert_eq!(judge_abema(&page(200, "", "{}")).status, "region_blocked");
        let device = page(200, "", r#"{"deviceid":"abc"}"#);
        let token = page(200, "", r#"{"animeSn":37783}"#);
        let home = page(200, "", r#"data-geo="TW""#);
        assert_eq!(judge_bahamut(&device, Some(&token), Some(&home)).region, "TW");
        assert_eq!(judge_bahamut(&device, Some(&page(200, "", "no")), None).status, "region_blocked");
        assert_eq!(judge_bbc(&page(200, "", "vs-hls-push-uk")).region, "GB");
        assert_eq!(judge_bbc(&page(200, "", "geolocation")).status, "region_blocked");
        assert_eq!(judge_iqiyi(&headed(200, "https://www.iq.com/", "", "mod=ntw; Path=/")).region, "TW");
        assert_eq!(judge_iqiyi(&page(200, "https://www.iq.com/", "welcome")).status, "unknown");
        assert_eq!(judge_kktv(&page(200, "", r#"{"country":"TW"}"#)).status, "available");
        assert_eq!(judge_kktv(&page(200, "", r#"{"country":"JP"}"#)).status, "region_blocked");
        assert_eq!(judge_viu(&page(200, "https://www.viu.com/ott/hk/zh", "")).region, "HK");
        assert_eq!(judge_viu(&page(200, "https://www.viu.com/ott/NO-SERVICE", "")).status, "region_blocked");
        assert_eq!(judge_mytvsuper(&page(200, "", r#"{"country_code":"HK"}"#)).status, "available");
        assert_eq!(judge_mytvsuper(&page(200, "", r#"{"country_code":"SG"}"#)).status, "region_blocked");
        assert_eq!(judge_apple(&page(200, "", "US")).region, "US");
        assert_eq!(judge_apple(&page(200, "", "<html>")).status, "unknown");
        assert_eq!(judge_wikipedia(&page(200, "", "Edit Wikipedia")).status, "available");
        assert_eq!(judge_wikipedia(&page(200, "", "You are Banned")).status, "region_blocked");
        assert_eq!(judge_reddit(&page(200, "https://www.reddit.com/", "")).status, "available");
        assert_eq!(judge_reddit(&page(403, "https://www.reddit.com/", "")).status, "region_blocked");
        assert_eq!(judge_reddit(&page(200, "https://www.reddit.com/", "Just a moment")).status, "unknown");
        assert_eq!(judge_meta(&page(200, "", r#"KadabraRootContainer "code":"en_US""#)).region, "US");
        assert_eq!(judge_meta(&page(200, "", "AbraGeoBlockedErrorRoot")).status, "region_blocked");
        assert_eq!(judge_meta(&page(200, "", "meta")).status, "unknown");
        assert_eq!(judge_page(&page(200, "", "Welcome to Copilot"), &["copilot"], &["not available in your region"]).status, "available");
    }
}
