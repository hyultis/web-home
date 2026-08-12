use axum::body::Bytes;
use axum::extract::{Request,State};
use axum::http::{HeaderMap,HeaderValue,StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use Htrace::components::level::Level;
use Htrace::HTrace;
use leptos::config::ReloadWSProtocol;
use leptos::nonce::Nonce;
use leptos::prelude::{Env,LeptosOptions};
use serde::Deserialize;
use std::sync::{LazyLock,Mutex};
use std::time::{Duration,Instant};
use url::Url;

#[derive(Clone)]
pub(super) struct BrowserContentSecurity
{
	liveReload: Option<BrowserContentSecurityLiveReload>,
}

#[derive(Clone)]
struct BrowserContentSecurityLiveReload
{
	protocol: &'static str,
	port: u32,
}

impl BrowserContentSecurity
{
	pub(super) const REPORT_PATH: &'static str = "/csp-report";
	pub(super) const REPORT_BODY_MAXIMUM_BYTES: usize = 16 * 1024;
	const REPORTING_ENDPOINTS: &'static str = "webhome-csp=\"/csp-report\"";
	const REPORT_LOG_MAXIMUM_PER_MINUTE: u16 = 64;

	pub(super) fn new(options: &LeptosOptions,watchEnabled: bool) -> Self
	{
		let port = options.reload_external_port.unwrap_or(options.reload_port);
		let liveReload = (options.env == Env::DEV && watchEnabled && port > 0 && port <= u16::MAX as u32)
			.then(|| BrowserContentSecurityLiveReload {
				protocol: match options.reload_ws_protocol
				{
					ReloadWSProtocol::WS => "ws",
					ReloadWSProtocol::WSS => "wss",
				},
				port,
			});
		return Self {liveReload};
	}

	pub(super) async fn headers_apply(
		State(contentSecurity): State<Self>,
		mut request: Request,
		next: Next,
	) -> Response
	{
		let nonce = Nonce::new();
		let policyHeader = contentSecurity.policyHeader_get(&nonce,request.headers());
		request.extensions_mut().insert(nonce);
		let mut response = next.run(request).await;
		contentSecurity.headers_insert(response.headers_mut(),policyHeader);

		return response;
	}

	pub(super) async fn report_receive(headers: HeaderMap,body: Bytes) -> StatusCode
	{
		if (!Self::reportContentType_isAccepted(&headers))
		{
			Self::reportRejected_log("unsupported-content-type");
			return StatusCode::UNSUPPORTED_MEDIA_TYPE;
		}
		let Some(violations) = Self::violations_parse(&body)
		else
		{
			Self::reportRejected_log("invalid-json-contract");
			return StatusCode::BAD_REQUEST;
		};

		for violation in violations
		{
			if (!CspReportLogWindow::permit_take(Self::REPORT_LOG_MAXIMUM_PER_MINUTE))
			{
				break;
			}
			HTrace!((Level::WARNING)
				"Browser CSP report directive={} blocked={} source={} line={} column={}",
				violation.directiveForLog_get(),
				violation.blockedForLog_get(),
				violation.sourceForLog_get(),
				violation.lineNumber.unwrap_or_default(),
				violation.columnNumber.unwrap_or_default()
			);
		}
		return StatusCode::NO_CONTENT;
	}

	pub(super) fn reportPath_is(path: &str) -> bool
	{
		return path == Self::REPORT_PATH;
	}

	fn headers_insert(&self,headers: &mut HeaderMap,policyHeader: HeaderValue)
	{
		use http::header::*;

		headers.insert(X_FRAME_OPTIONS,HeaderValue::from_static("DENY"));
		headers.insert(CONTENT_SECURITY_POLICY,policyHeader);
		headers.insert(
			HeaderName::from_static("reporting-endpoints"),
			HeaderValue::from_static(Self::REPORTING_ENDPOINTS),
		);
		headers.insert(X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff"));
		headers.insert(STRICT_TRANSPORT_SECURITY,HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"));
		headers.insert(REFERRER_POLICY,HeaderValue::from_static("no-referrer"));
	}

	fn policyHeader_get(&self,nonce: &Nonce,requestHeaders: &HeaderMap) -> HeaderValue
	{
		let liveReloadSource = self.liveReloadSource_get(requestHeaders)
			.map(|source| format!(" {source}"))
			.unwrap_or_default();
		let policy = format!(
			concat!(
				"default-src 'none'; ",
				"base-uri 'none'; ",
				"connect-src 'self' https://api.open-meteo.com{}; ",
				"font-src 'none'; ",
				"form-action 'self'; ",
				"frame-ancestors 'none'; ",
				"frame-src 'self'; ",
				"img-src 'self' data: blob: http: https:; ",
				"manifest-src 'self'; ",
				"media-src 'none'; ",
				"object-src 'none'; ",
				"script-src 'self' 'nonce-{}' 'wasm-unsafe-eval'; ",
				"script-src-attr 'none'; ",
				"style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; ",
				"worker-src 'self'; ",
				"report-uri /csp-report; ",
				"report-to webhome-csp",
			),
			liveReloadSource,
			nonce,
		);
		return HeaderValue::from_str(&policy).expect("generated CSP header must be valid");
	}

	fn liveReloadSource_get(&self,requestHeaders: &HeaderMap) -> Option<String>
	{
		use http::header::HOST;

		let liveReload = self.liveReload.as_ref()?;
		let rawAuthority = requestHeaders.get(HOST)?.to_str().ok()?;
		if (rawAuthority.contains('@'))
		{
			return None;
		}
		let authority = rawAuthority.parse::<http::uri::Authority>().ok()?;
		let host = authority.host();
		if (host.is_empty())
		{
			return None;
		}
		return Some(format!("{}://{}:{}",liveReload.protocol,host,liveReload.port));
	}

	fn reportContentType_isAccepted(headers: &HeaderMap) -> bool
	{
		use http::header::CONTENT_TYPE;

		let Some(contentType) = headers.get(CONTENT_TYPE).and_then(|value| value.to_str().ok())
		else
		{
			return false;
		};
		let contentType = contentType.split(';').next().unwrap_or_default().trim();
		return ["application/csp-report","application/reports+json"].iter()
			.any(|accepted| contentType.eq_ignore_ascii_case(accepted));
	}

	fn violations_parse(body: &[u8]) -> Option<Vec<CspViolation>>
	{
		if let Ok(report) = serde_json::from_slice::<LegacyCspReport>(body)
		{
			return Some(vec![report.violation]);
		}
		if let Ok(reports) = serde_json::from_slice::<Vec<ReportingApiReport>>(body)
		{
			let violations = reports.into_iter()
				.filter(|report| report.reportType == "csp-violation")
				.map(|report| report.body)
				.collect::<Vec<_>>();
			return (!violations.is_empty()).then_some(violations);
		}
		return None;
	}

	fn reportRejected_log(reason: &str)
	{
		if (CspReportLogWindow::permit_take(Self::REPORT_LOG_MAXIMUM_PER_MINUTE))
		{
			HTrace!((Level::WARNING) "Browser CSP report rejected reason={}",reason);
		}
	}
}

#[derive(Deserialize)]
struct LegacyCspReport
{
	#[serde(rename="csp-report")]
	violation: CspViolation,
}

#[derive(Deserialize)]
struct ReportingApiReport
{
	#[serde(rename="type")]
	reportType: String,
	body: CspViolation,
}

#[derive(Default,Deserialize)]
#[serde(default)]
struct CspViolation
{
	#[serde(rename="effective-directive",alias="effectiveDirective")]
	effectiveDirective: String,
	#[serde(rename="blocked-uri",alias="blockedURL")]
	blockedUrl: String,
	#[serde(rename="source-file",alias="sourceFile")]
	sourceFile: String,
	#[serde(rename="line-number",alias="lineNumber")]
	lineNumber: Option<u64>,
	#[serde(rename="column-number",alias="columnNumber")]
	columnNumber: Option<u64>,
}

impl CspViolation
{
	fn directiveForLog_get(&self) -> String
	{
		return Self::tokenForLog_get(&self.effectiveDirective,"unknown-directive");
	}

	fn blockedForLog_get(&self) -> String
	{
		return Self::sourceValueForLog_get(&self.blockedUrl);
	}

	fn sourceForLog_get(&self) -> String
	{
		return Self::sourceValueForLog_get(&self.sourceFile);
	}

	fn sourceValueForLog_get(raw: &str) -> String
	{
		if (raw.is_empty())
		{
			return "none".to_string();
		}
		if (raw.len() <= 2048)
		{
			if let Ok(url) = Url::parse(raw)
			{
				return match url.scheme()
				{
					"http" | "https" => url.origin().ascii_serialization(),
					"blob" | "data" => format!("{}:",url.scheme()),
					_ => "redacted-scheme".to_string(),
				};
			}
		}
		return match raw
		{
			"inline" | "eval" | "wasm-eval" | "self" => raw.to_string(),
			_ => "redacted-source".to_string(),
		};
	}

	fn tokenForLog_get(raw: &str,fallback: &str) -> String
	{
		let token = raw.chars()
			.filter(|character| character.is_ascii_alphanumeric() || matches!(character,'-' | '_'))
			.take(64)
			.collect::<String>();
		return (!token.is_empty()).then_some(token).unwrap_or_else(|| fallback.to_string());
	}
}

struct CspReportLogWindow
{
	startedAt: Instant,
	accepted: u16,
}

impl CspReportLogWindow
{
	fn permit_take(maximumPerMinute: u16) -> bool
	{
		static WINDOW: LazyLock<Mutex<CspReportLogWindow>> = LazyLock::new(|| Mutex::new(CspReportLogWindow {
			startedAt: Instant::now(),
			accepted: 0,
		}));

		let Ok(mut window) = WINDOW.lock()
		else
		{
			return false;
		};
		if (window.startedAt.elapsed() >= Duration::from_secs(60))
		{
			window.startedAt = Instant::now();
			window.accepted = 0;
		}
		if (window.accepted >= maximumPerMinute)
		{
			return false;
		}
		window.accepted += 1;
		return true;
	}
}

#[cfg(test)]
mod tests
{
	use super::{BrowserContentSecurity,CspViolation};
	use axum::body::{to_bytes,Body};
	use axum::extract::Extension;
	use axum::http::{HeaderMap,HeaderValue,Request};
	use axum::middleware;
	use axum::routing::get;
	use axum::Router;
	use leptos::config::ReloadWSProtocol;
	use leptos::nonce::Nonce;
	use leptos::prelude::{Env,LeptosOptions};
	use tower::ServiceExt;

	#[tokio::test]
	async fn middlewareAddsEnforcedPolicyUsingRequestNonce()
	{
		let contentSecurity = contentSecurity_get(Env::PROD,false);
		let router = Router::new()
			.route("/",get(|Extension(nonce): Extension<Nonce>| async move {nonce.to_string()}))
			.layer(middleware::from_fn_with_state(contentSecurity,BrowserContentSecurity::headers_apply));
		let response = router.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
		assert_eq!(response.headers().get("reporting-endpoints").unwrap(),"webhome-csp=\"/csp-report\"");
		assert!(response.headers().get("content-security-policy-report-only").is_none());
		let policy = response.headers().get("content-security-policy").unwrap().to_str().unwrap().to_string();
		let nonce = String::from_utf8(to_bytes(response.into_body(),128).await.unwrap().to_vec()).unwrap();

		assert!(policy.contains(&format!("'nonce-{nonce}'")));
	}

	#[tokio::test]
	async fn leptosShellUsesSameRequestNonceAsHeader()
	{
		// Route generation performs the same executor initialization as the production router.
		let _ = leptos_axum::generate_route_list(|| "test");
		let mut options = leptos::prelude::get_configuration(Some("Cargo.toml")).unwrap().leptos_options;
		let hashPath = std::env::temp_dir().join(format!("webhome-shell-hash-{}.txt",std::process::id()));
		std::fs::write(&hashPath,"js: test-js-hash\nwasm: test-wasm-hash\ncss: test-css-hash\n").unwrap();
		options.hash_files = true;
		options.hash_file = hashPath.to_string_lossy().into_owned().into();
		let contentSecurity = BrowserContentSecurity::new(&options,std::env::var_os("LEPTOS_WATCH").is_some());
		let handler = leptos_axum::render_app_to_stream_in_order_with_context(
			|| {},
			move || web_home::entry::shell((options.clone(),false,false)),
		);
		let router = Router::new()
			.route("/",get(handler))
			.layer(middleware::from_fn_with_state(contentSecurity,BrowserContentSecurity::headers_apply));
		let response = router.oneshot(Request::builder().uri("/").header("host","127.0.0.1:3002").body(Body::empty()).unwrap()).await.unwrap();
		let policy = response.headers().get("content-security-policy").unwrap().to_str().unwrap().to_string();
		let nonceStart = policy.find("'nonce-").unwrap() + "'nonce-".len();
		let nonceEnd = policy[nonceStart..].find('\'').unwrap() + nonceStart;
		let nonce = &policy[nonceStart..nonceEnd];
		let body = String::from_utf8(to_bytes(response.into_body(),2 * 1024 * 1024).await.unwrap().to_vec()).unwrap();
		std::fs::remove_file(hashPath).unwrap();

		let inlineScriptTags = inlineScriptTags_get(&body);
		assert!(!inlineScriptTags.is_empty(),"Leptos shell does not contain an inline hydration script");
		for tag in inlineScriptTags
		{
			assert!(
				tag.contains(&format!("nonce=\"{nonce}\"")) || tag.contains(&format!("nonce={nonce}")),
				"Leptos inline script does not use the CSP response nonce: {tag}",
			);
		}
		if (std::env::var_os("LEPTOS_WATCH").is_some())
		{
			assert!(body.contains("/live_reload"),"LEPTOS_WATCH does not render AutoReload");
		}
		assert!(body.contains("https://cdn.jsdelivr.net/npm/iconoir@7.11.1/css/iconoir.css"));
		assert!(body.contains("sha384-luECWXGw+Rk0LDPKZ8m2vuzYJnGiJfFabF16BAqKVf7rdp1/jvaViZ+BFXFuaD5H"));
		assert!(body.contains("crossorigin=\"anonymous\"") || body.contains("crossorigin=anonymous"));
		assert!(!body.contains("iconoir@main"));
		assert!(body.contains("/pkg/webhome.test-css-hash.css"));
		assert!(body.contains("/pkg/webhome.test-js-hash.js"));
		assert!(body.contains("/pkg/webhome.test-wasm-hash.wasm"));
		assert!(!body.contains("/pkg/webhome.css"));
		assert!(!body.contains("/pkg/webhome.js"));
		assert!(!body.contains("/pkg/webhome.wasm"));
		let assetMonitorPosition = body.find("asset_version_monitor.js").unwrap();
		let hydrationAssetPosition = body.find("/pkg/webhome.test-js-hash.js").unwrap();
		assert!(assetMonitorPosition < hydrationAssetPosition,"asset version monitor must wrap fetch before hydration starts");
	}

	fn inlineScriptTags_get(body: &str) -> Vec<&str>
	{
		let mut tags = Vec::new();
		let mut remaining = body;
		while let Some(start) = remaining.find("<script")
		{
			remaining = &remaining[start..];
			let Some(end) = remaining.find('>')
			else
			{
				break;
			};
			let tag = &remaining[..=end];
			if (!tag.contains("src="))
			{
				tags.push(tag);
			}
			remaining = &remaining[end + 1..];
		}
		return tags;
	}

	#[test]
	fn enforcedPolicyContainsRequiredBoundariesAndNonce()
	{
		let contentSecurity = contentSecurity_get(Env::PROD,true);
		let policy = policy_get(&contentSecurity,"home.example");
		let policy = policy.to_str().unwrap();

		for directive in [
			"default-src 'none'",
			"connect-src 'self' https://api.open-meteo.com",
			"object-src 'none'",
			"script-src 'self' 'nonce-",
			"style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net",
			"worker-src 'self'",
			"report-uri /csp-report",
			"report-to webhome-csp",
		]
		{
			assert!(policy.contains(directive),"missing CSP directive: {directive}");
		}
		assert!(!policy.contains("'unsafe-eval'"));
		assert!(!policy.contains("ws://"));
		assert!(!policy.contains("wss://"));
	}

	#[test]
	fn developmentWatchAllowsOnlyConfiguredReloadOrigin()
	{
		let contentSecurity = contentSecurity_get(Env::DEV,true);
		let policy = policy_get(&contentSecurity,"127.0.0.1:3002");
		let policy = policy.to_str().unwrap();

		assert!(policy.contains("connect-src 'self' https://api.open-meteo.com ws://127.0.0.1:3011;"));
		assert_eq!(policy.matches("ws://").count(),1);
		assert!(!policy.contains("wss://"));
	}

	#[test]
	fn developmentWithoutWatchDoesNotAllowReloadOrigin()
	{
		let contentSecurity = contentSecurity_get(Env::DEV,false);
		let policy = policy_get(&contentSecurity,"127.0.0.1:3002");
		let policy = policy.to_str().unwrap();

		assert!(!policy.contains("ws://"));
		assert!(!policy.contains("wss://"));
	}

	#[test]
	fn developmentWatchUsesExternalPortAndWssConfiguration()
	{
		let mut options = leptosOptions_get(Env::DEV);
		options.reload_external_port = Some(443);
		options.reload_ws_protocol = ReloadWSProtocol::WSS;
		let contentSecurity = BrowserContentSecurity::new(&options,true);
		let policy = policy_get(&contentSecurity,"dev.example:8443");
		let policy = policy.to_str().unwrap();

		assert!(policy.contains("wss://dev.example:443"));
		assert!(!policy.contains("ws://"));
		assert!(!policy.contains(":3011"));
	}

	#[test]
	fn developmentWatchFailsClosedWithoutValidRequestHost()
	{
		let contentSecurity = contentSecurity_get(Env::DEV,true);
		let policyWithoutHost = contentSecurity.policyHeader_get(&Nonce::new(),&HeaderMap::new());
		let policyWithUserInfo = policy_get(&contentSecurity,"user@dev.example:3002");

		for policy in [policyWithoutHost,policyWithUserInfo]
		{
			let policy = policy.to_str().unwrap();
			assert!(!policy.contains("ws://"));
			assert!(!policy.contains("wss://"));
		}
	}

	fn contentSecurity_get(environment: Env,watchEnabled: bool) -> BrowserContentSecurity
	{
		return BrowserContentSecurity::new(&leptosOptions_get(environment),watchEnabled);
	}

	fn leptosOptions_get(environment: Env) -> LeptosOptions
	{
		let mut options = leptos::prelude::get_configuration(Some("Cargo.toml")).unwrap().leptos_options;
		options.env = environment;
		options.reload_port = 3011;
		options.reload_external_port = None;
		options.reload_ws_protocol = ReloadWSProtocol::WS;
		return options;
	}

	fn policy_get(contentSecurity: &BrowserContentSecurity,host: &'static str) -> HeaderValue
	{
		let mut headers = HeaderMap::new();
		headers.insert("host",HeaderValue::from_static(host));
		return contentSecurity.policyHeader_get(&Nonce::new(),&headers);
	}

	#[test]
	fn legacyReportParsesAndRemovesUrlPathsAndCredentialsFromLogs()
	{
		let reports = BrowserContentSecurity::violations_parse(br#"{
			"csp-report": {
				"effective-directive": "script-src-elem",
				"blocked-uri": "https://user:secret@cdn.example/private.js?token=secret",
				"source-file": "https://home.example/account/private",
				"line-number": 12,
				"column-number": 3
			}
		}"#).unwrap();

		assert_eq!(reports.len(),1);
		assert_eq!(reports[0].directiveForLog_get(),"script-src-elem");
		assert_eq!(reports[0].blockedForLog_get(),"https://cdn.example");
		assert_eq!(reports[0].sourceForLog_get(),"https://home.example");
	}

	#[test]
	fn reportingApiArrayParsesOnlyCspViolations()
	{
		let reports = BrowserContentSecurity::violations_parse(br#"[
			{"type":"network-error","body":{}},
			{"type":"csp-violation","body":{"effectiveDirective":"worker-src","blockedURL":"blob:https://home.example/id"}}
		]"#).unwrap();

		assert_eq!(reports.len(),1);
		assert_eq!(reports[0].directiveForLog_get(),"worker-src");
		assert_eq!(reports[0].blockedForLog_get(),"blob:");
	}

	#[test]
	fn reportContentTypeAcceptsOnlyCspJsonFormats()
	{
		let mut headers = HeaderMap::new();
		assert!(!BrowserContentSecurity::reportContentType_isAccepted(&headers));
		headers.insert("content-type",HeaderValue::from_static("application/csp-report"));
		assert!(BrowserContentSecurity::reportContentType_isAccepted(&headers));
		headers.insert("content-type",HeaderValue::from_static("application/reports+json; charset=utf-8"));
		assert!(BrowserContentSecurity::reportContentType_isAccepted(&headers));
		headers.insert("content-type",HeaderValue::from_static("Application/CSP-Report"));
		assert!(BrowserContentSecurity::reportContentType_isAccepted(&headers));
		headers.insert("content-type",HeaderValue::from_static("application/json"));
		assert!(!BrowserContentSecurity::reportContentType_isAccepted(&headers));
	}

	#[test]
	fn reportLogTokensRemoveControlsAndPunctuation()
	{
		let violation = CspViolation {
			effectiveDirective: "script-src\n forged=value".to_string(),
			..Default::default()
		};
		assert_eq!(violation.directiveForLog_get(),"script-srcforgedvalue");
	}
}
