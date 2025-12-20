#![allow(unused_parens)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(non_camel_case_types)]

use std::sync::atomic::AtomicBool;
use axum::middleware;
use Hconfig::IO::json::WrapperJson;
use Htrace::HTraceError;
use time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use web_home::entry::AppProps;
use crate::api::IS_PROD;
use crate::api::proxys::proxy_cache::CACHE_DIR;
use crate::global_security::generate_salt;

mod api;
pub mod global_security;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
	use std::fs;
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

	let _ = fs::create_dir("./config");
	let _ = fs::create_dir("./config/users");
	let _ = fs::create_dir("./dynamic");
	let _ = fs::create_dir(CACHE_DIR);
	let _ = fs::remove_dir_all("./dynamic/traces");

	HConfigManager::singleton().confPath_set("./config");
	HConfigManager::singleton()
		.create::<WrapperJson>("htrace")
		.expect("bug from hconfig");
	HConfigManager::singleton()
		.create::<WrapperJson>("site")
		.expect("bug from hconfig");

	// set default site config
	if let Some(mut siteConfig) = HConfigManager::singleton().get("site")
	{
		let config = siteConfig.value_mut();
		helper::preFillConfig(config,"salt",generate_salt().expect("Cannot generate a salt for website (site.json/salt)"));
		helper::preFillConfig(config,"allow_registration",true);
		HTraceError!(config.file_save());
	}

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
	let _ = IS_PROD.set(AtomicBool::new(conf.leptos_options.env==Env::PROD));
	HTracer::globalContext_set(global_context);
	HTrace!((Level::DEBUG) "leptos option env : {:?}",conf.leptos_options.env);

	//conf.leptos_options.site_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 3000);
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options.clone();

	//session management
	let session_store = MemoryStore::default();
	let session_layer = SessionManagerLayer::new(session_store)
		.with_secure(true)
		.with_expiry(Expiry::OnInactivity(Duration::days(1)));

	let leptos_options_inner_app = leptos_options.clone();
    let app = Router::new()
        .leptos_routes(&leptos_options, generate_route_list(move || {
	        let leptos_options = leptos_options_inner_app.clone();
	        App(AppProps { isProd: leptos_options.env == Env::PROD })
        }), {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
	    .layer(middleware::from_fn(helper::tracing_request))
	    .layer(middleware::from_fn(helper::http_good_practice))
	    .layer(session_layer)
        .with_state(leptos_options);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    HTrace!((Level::DEBUG) "listening on http://{}", &addr);
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
	use http::header::*;

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


		let response = next.run(request).await;

		if(!(uri.contains("API_translate_getBook") || uri.contains("API_Htrace_log")))
		{
			HTrace!("Request {} on {} : {}", method, uri, response.status());
		}

		response
	}


	pub(crate) async fn http_good_practice(
		request: Request,
		next: Next,
	) -> Response {
		let mut response = next.run(request).await;

		response.headers_mut().insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
		response.headers_mut().insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static("frame-ancestors 'none'"));
		response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
		response.headers_mut().insert(STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"));
		response.headers_mut().insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));

		response
	}
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
	// no client-side main function
	// unless we want this to work with e.g., Trunk for pure client-side testing
	// see lib.rs for hydration function instead
}