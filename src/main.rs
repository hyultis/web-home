#![allow(unused_parens)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

use axum::middleware;
use Hconfig::IO::json::WrapperJson;
use Hconfig::tinyjson::JsonValue;
use Htrace::HTraceError;
use web_home::entry::AppProps;
use web_home::global_security::generate_salt;
use web_home::server::{
	runtimeConfig_set,
	passwordRotationBodyLimit_apply,
	sessionErrorActivity_renew,
	sessionLayer_get,
	traceFrontLog_enabled,
	PROXY_CACHE_DIR,
};
#[cfg(feature = "ssr")]
use crate::browser_asset_delivery::BrowserAssetDelivery;
#[cfg(feature = "ssr")]
use crate::browser_content_security::BrowserContentSecurity;
#[cfg(feature = "ssr")]
use crate::deployment_health::DeploymentHealth;
#[cfg(feature = "ssr")]
mod browser_asset_delivery;
#[cfg(feature = "ssr")]
mod browser_content_security;
#[cfg(feature = "ssr")]
mod deployment_health;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
	use std::fs;
	use axum::extract::DefaultBodyLimit;
	use axum::routing::{get, post};
	use axum::Router;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
	use Hconfig::HConfigManager::HConfigManager;
	use web_home::entry::{shell, App};
	use Htrace::modules::command_line::CommandLine;
	use Htrace::modules::command_line_config::CommandLineConfig;
	use Htrace::modules::file::File;
	use Htrace::modules::file_config::FileConfig;
	use Htrace::htracer::HTracer;
	use Htrace::components::level::Level;
	use Htrace::components::context::Context;
	use Htrace::HTrace;

	let mut conf = get_configuration(None).unwrap();
	// redefining ENV options from ENV if existing
	if let Ok(env) = std::env::var("ENV")
	{
		if(env=="PROD")
		{
			conf.leptos_options.env = Env::PROD
		}
	}
	let production = conf.leptos_options.env == Env::PROD;

	let _ = fs::create_dir("./config");
	let _ = fs::create_dir("./config/users");
	let _ = fs::create_dir("./dynamic");
	let _ = fs::create_dir(PROXY_CACHE_DIR);
	let _ = fs::remove_dir_all("./dynamic/traces");

	let mut global_context = Context::default();
	global_context.module_add("cmd",CommandLine::new(CommandLineConfig::default()));
	global_context.module_add("file", File::new(FileConfig{
		path: "./dynamic/traces".to_string(),
		bySrc: true,
		byThreadId: false,
		..Default::default()

	}));
	global_context.level_setMin(Some(Level::DEBUG));
	if(conf.leptos_options.env==Env::PROD)
	{
		global_context.level_setMin(Some(Level::NOTICE));
	}
	HTracer::globalContext_set(global_context);

	HConfigManager::singleton().confPath_set("./config");
	HConfigManager::singleton()
		.create::<WrapperJson>("site")
		.expect("bug from hconfig");

	// set default site config
	let mut trace_front_log = false;
	let mut allow_registration = false;
	if let Some(mut siteConfig) = HConfigManager::singleton().get("site")
	{
		let config = siteConfig.value_mut();
		helper::preFillConfig(config,"salt",generate_salt().expect("Cannot generate a salt for website (site.json/salt)"));
		helper::preFillConfig(config,"allow_registration",true);
		helper::preFillConfig(config,"trace_front_log",!production);
		helper::preFillConfig(config,"imap_allowed_ports",vec![JsonValue::Number(993.0)]);
		if let Some(JsonValue::Boolean(raw)) = config.value_get("trace_front_log")
		{
			trace_front_log = traceFrontLog_enabled(raw,production);
		}
		if let Some(JsonValue::Boolean(raw)) = config.value_get("allow_registration")
		{
			allow_registration = raw;
		}
		HTraceError!(config.file_save());
	}

	HTrace!((Level::DEBUG) "leptos option env : {:?}",conf.leptos_options.env);
	HTrace!((Level::DEBUG) "is IS_TRACE_FRONT_LOG ? : {:?}",trace_front_log);
	HTrace!((Level::DEBUG) "is ALLOW_REGISTRATION ? : {:?}",allow_registration);
	runtimeConfig_set(trace_front_log,allow_registration);

	//conf.leptos_options.site_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 3000);
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options.clone();
	let browserContentSecurity = BrowserContentSecurity::new(
		&leptos_options,
		std::env::var_os("LEPTOS_WATCH").is_some(),
	);
	let browserAssetDelivery = BrowserAssetDelivery::new(&leptos_options);

	//session management
	let session_layer = sessionLayer_get();

	let app = Router::new()
		.leptos_routes(&leptos_options, generate_route_list(move || {
			App(AppProps { traceFrontLog: trace_front_log, allowRegistration: allow_registration })
        }), {
            let leptos_options = leptos_options.clone();
            move || shell((leptos_options.clone(),trace_front_log,allow_registration))
        })
	    .fallback(leptos_axum::file_and_error_handler(move |lo|shell((lo,trace_front_log,allow_registration))))
	    .layer(middleware::from_fn(sessionErrorActivity_renew))
	    .layer(middleware::from_fn(passwordRotationBodyLimit_apply))
	    .layer(session_layer)
	    .route(DeploymentHealth::PATH, get(DeploymentHealth::response_get))
	    .route(
		    BrowserContentSecurity::REPORT_PATH,
		    post(BrowserContentSecurity::report_receive)
			    .layer(DefaultBodyLimit::max(BrowserContentSecurity::REPORT_BODY_MAXIMUM_BYTES)),
	    )
	    .layer(middleware::from_fn_with_state(
		browserAssetDelivery,
		BrowserAssetDelivery::headers_apply,
	    ))
	    .layer(middleware::from_fn(helper::tracing_request))
	    .layer(middleware::from_fn_with_state(
		browserContentSecurity,
		BrowserContentSecurity::headers_apply,
	    ))
        .with_state(leptos_options);

    // to run our app
    HTrace!((Level::DEBUG) "listening on http(s)://{}", &addr);
	let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
	axum::serve(listener, app.into_make_service()).await.unwrap();
}

#[cfg(feature = "ssr")]
mod helper {
	use axum::extract::Request;
	use axum::middleware::Next;
	use axum::response::Response;
	use Hconfig::HConfig::HConfig;
	use Hconfig::tinyjson::JsonValue;

	pub fn preFillConfig(config: &mut HConfig,fieldName: impl Into<String>, data: impl Into<JsonValue>)
	{
		let fieldName = fieldName.into();
		if match config.value_get(&fieldName) {
			None => true,
			Some(JsonValue::String(ref content)) if content.is_empty() => true,
			Some(_) => false
		} {
			config.value_set(&fieldName,data);
		}
	}

	pub(crate) async fn tracing_request(
		request: Request,
		next: Next,
	) -> Response {
		use Htrace::HTrace;

		let method = request.method().to_string();
		let uri = request.uri().to_string();
		let isCspReport = super::BrowserContentSecurity::reportPath_is(request.uri().path());
		let isDeploymentHealth = super::DeploymentHealth::path_is(request.uri().path());


		let response = next.run(request).await;

		if(!(uri.contains("API_translate_getBook") || uri.contains("API_Htrace_log") || isCspReport || isDeploymentHealth))
		{
			HTrace!("Request {} on {} : {}", method, uri, response.status());
		}

		response
	}
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
	// no client-side main function
	// unless we want this to work with e.g., Trunk for pure client-side testing
	// see lib.rs for hydration function instead
}

#[cfg(test)]
mod crateOwnership_contractTests
{
	const MAIN_SOURCE: &str = include_str!("main.rs");

	#[test]
	fn binaryImportsLibraryDomainsWithoutRedeclaringThem()
	{
		assert!(!MAIN_SOURCE.lines().any(|line| line.trim() == "mod api;"));
		assert!(!MAIN_SOURCE.lines().any(|line| line.trim() == "pub mod global_security;"));
		assert!(MAIN_SOURCE.contains("use web_home::global_security::generate_salt;"));
		assert!(MAIN_SOURCE.contains("use web_home::server::{"));
	}

	#[test]
	fn startupInitializesHtraceBeforeFirstTrace()
	{
		let initializationPosition = MAIN_SOURCE.find("HTracer::globalContext_set(global_context);").unwrap();
		let firstErrorTracePosition = MAIN_SOURCE.find("HTraceError!(").unwrap();
		let firstTracePosition = MAIN_SOURCE.find("HTrace!((").unwrap();

		assert!(initializationPosition < firstErrorTracePosition);
		assert!(initializationPosition < firstTracePosition);
	}
}
