use gloo_timers::callback::Timeout;
use js_sys::Uint8Array;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

use crate::api::proxys::imap_components::Attachment;

struct AttachmentDownload
{
	filename: String,
	contentType: String,
	data: Vec<u8>,
}

impl AttachmentDownload
{
	const DATA_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const FILENAME_MAXIMUM_CHARACTERS: usize = 255;
	const MIME_MAXIMUM_BYTES: usize = 255;

	fn new(attachment: Attachment) -> Option<Self>
	{
		if (attachment.data.len() > Self::DATA_MAXIMUM_BYTES)
		{
			return None;
		}
		let filename = attachment.filename
			.as_deref()
			.map(Self::filename_sanitize)
			.filter(|filename| !filename.is_empty())
			.unwrap_or_else(|| "download".to_string());
		let contentType = Self::contentType_sanitize(&attachment.content_type);
		return Some(Self {filename,contentType,data: attachment.data});
	}

	fn download(self) -> bool
	{
		let uint8Array = Uint8Array::from(self.data.as_slice());
		let blobOptions = BlobPropertyBag::new();
		blobOptions.set_type(&self.contentType);
		let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(
			&js_sys::Array::of1(&uint8Array),
			&blobOptions,
		)
		else
		{
			return false;
		};
		let Ok(url) = Url::create_object_url_with_blob(&blob)
		else
		{
			return false;
		};
		let Some(window) = web_sys::window()
		else
		{
			let _ = Url::revoke_object_url(&url);
			return false;
		};
		let Some(document) = window.document()
		else
		{
			let _ = Url::revoke_object_url(&url);
			return false;
		};
		let Ok(element) = document.create_element("a")
		else
		{
			let _ = Url::revoke_object_url(&url);
			return false;
		};
		let Ok(anchor) = element.dyn_into::<HtmlAnchorElement>()
		else
		{
			let _ = Url::revoke_object_url(&url);
			return false;
		};
		anchor.set_href(&url);
		anchor.set_download(&self.filename);
		let _ = anchor.style().set_property("display","none");
		let Some(body) = document.body()
		else
		{
			let _ = Url::revoke_object_url(&url);
			return false;
		};
		if (body.append_child(&anchor).is_err())
		{
			let _ = Url::revoke_object_url(&url);
			return false;
		}
		anchor.click();
		let _ = body.remove_child(&anchor);
		Timeout::new(1_000,move || {
			let _ = Url::revoke_object_url(&url);
		}).forget();
		return true;
	}

	fn filename_sanitize(filename: &str) -> String
	{
		return filename.chars()
			.filter(|character| !character.is_control())
			.map(|character| if (matches!(character,'/' | '\\' | ':')) {'_'} else {character})
			.take(Self::FILENAME_MAXIMUM_CHARACTERS)
			.collect::<String>()
			.trim()
			.trim_matches('.')
			.to_string();
	}

	fn contentType_sanitize(contentType: &str) -> String
	{
		if (contentType.is_empty()
			|| contentType.len() > Self::MIME_MAXIMUM_BYTES
			|| !contentType.is_ascii()
			|| contentType.chars().any(char::is_control)
			|| !contentType.contains('/'))
		{
			return "application/octet-stream".to_string();
		}
		return contentType.to_string();
	}
}

pub fn download_attachment(attachment: Attachment) -> bool
{
	return AttachmentDownload::new(attachment)
		.is_some_and(AttachmentDownload::download);
}

#[cfg(test)]
mod tests
{
	use super::AttachmentDownload;

	#[test]
	fn attachmentDownload_sanitizesFilenameAndMime()
	{
		assert_eq!(AttachmentDownload::filename_sanitize("../bad\\name:\0.txt"),"_bad_name_.txt");
		assert_eq!(AttachmentDownload::contentType_sanitize("text/plain"),"text/plain");
		assert_eq!(AttachmentDownload::contentType_sanitize("text/plain\nscript"),"application/octet-stream");
	}
}
