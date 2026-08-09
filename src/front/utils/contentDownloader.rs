use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, Url, HtmlAnchorElement};
use js_sys::Uint8Array;
use crate::api::proxys::imap_components::Attachment;

pub fn download_attachment(att: Attachment) -> bool {
	let filename = att
		.filename
		.clone()
		.unwrap_or_else(|| "download".to_string());

	// Vec<u8> -> Uint8Array
	let uint8_array = Uint8Array::from(att.data.as_slice());

	// Blob options
	let blob_opts = BlobPropertyBag::new();
	blob_opts.set_type(&att.content_type);

	// Création du Blob
	let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(
		&js_sys::Array::of1(&uint8_array),
		&blob_opts,
	) else {
		return false;
	};

	// Object URL
	let Ok(url) = Url::create_object_url_with_blob(&blob) else {
		return false;
	};

	// <a download>
	let Some(window) = web_sys::window() else {
		return false;
	};
	let Some(document) = window.document() else {
		return false;
	};
	let a = document
		.create_element("a")
		.unwrap()
		.dyn_into::<HtmlAnchorElement>()
		.unwrap();

	a.set_href(&url);
	a.set_download(&filename);
	let _ = a.style().set_property("display", "none");

	document.body().unwrap().append_child(&a).unwrap();
	a.click();

	// Cleanup
	document.body().unwrap().remove_child(&a).unwrap();
	Url::revoke_object_url(&url).unwrap();
	return true;
}
