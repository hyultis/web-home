use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;
use crate::HWebTrace;

async fn sync_weather_api()
{
	let url = format!(
		"https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly=temperature_2m",
		43.633721, 3.915183
	);

	let Ok(window) = web_sys::window().ok_or("no window") else {return};
	let Ok(resp_value) = JsFuture::from(window.fetch_with_str(&url)).await else {return};
	let Ok(resp): Result<Response,_> = resp_value.dyn_into() else {return};

	// On récupère le body en texte (JSON)
	let Ok(text_promise) = resp.text().map_err(|_| "text() failed") else {return};
	let Ok(text_js) = JsFuture::from(text_promise).await else {return};
	let Ok(text) = text_js
		.as_string()
		.ok_or("response text is not a string") else {return};
	HWebTrace!("api meteo : {:?}", text);
}