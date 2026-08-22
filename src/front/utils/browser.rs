#[cfg(feature = "hydrate")]
pub(crate) fn timezone_get() -> String
{
	use js_sys::{Array,Intl,Object,Reflect};
	let options = Intl::DateTimeFormat::new(&Array::new(),&Object::new()).resolved_options();
	return Reflect::get(&options,&wasm_bindgen::JsValue::from_str("timeZone")).ok()
		.and_then(|timezone| timezone.as_string())
		.filter(|timezone| !timezone.is_empty())
		.unwrap_or_else(|| "UTC".to_string());
}

#[cfg(not(feature = "hydrate"))]
pub(crate) fn timezone_get() -> String
{
	"UTC".to_string()
}
