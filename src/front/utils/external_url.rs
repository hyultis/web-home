use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::front) struct SafeExternalUrl
{
	normalized: String,
}

impl SafeExternalUrl
{
	pub(in crate::front) fn parse(rawUrl: &str) -> Option<Self>
	{
		if (rawUrl.is_empty() || rawUrl.chars().any(|character| character.is_whitespace() || character.is_control()))
		{
			return None;
		}
		let parsedUrl = Url::parse(rawUrl).ok()?;
		if (!matches!(parsedUrl.scheme(),"http" | "https") || parsedUrl.host_str().is_none())
		{
			return None;
		}
		return Some(Self {normalized: parsedUrl.to_string()});
	}

	pub(in crate::front) fn into_string(self) -> String
	{
		return self.normalized;
	}
}

#[cfg(test)]
mod tests
{
	use super::SafeExternalUrl;

	#[test]
	fn safeExternalUrl_acceptsAndNormalizesHttpSchemes()
	{
		let cases = [
			("http://example.com","http://example.com/"),
			("HTTPS://Example.COM/Path?value=1#part","https://example.com/Path?value=1#part"),
			("https://example.com/a%20b","https://example.com/a%20b"),
		];
		for (rawUrl,expected) in cases
		{
			let parsedUrl = SafeExternalUrl::parse(rawUrl).unwrap();
			assert_eq!(parsedUrl.into_string(),expected);
		}
	}

	#[test]
	fn safeExternalUrl_rejectsUnsupportedOrAmbiguousDestinations()
	{
		let cases = [
			"",
			"example.com",
			"/relative",
			"//example.com/path",
			"mailto:user@example.com",
			"javascript:alert(1)",
			"java\nscript:alert(1)",
			"data:text/html,unsafe",
			"file:///tmp/file",
			"ftp://example.com/file",
			"blob:https://example.com/id",
			" https://example.com",
			"https://example.com ",
			"https://example.com/a b",
			"https://",
		];
		for rawUrl in cases
		{
			assert_eq!(SafeExternalUrl::parse(rawUrl),None,"unexpected accepted URL: {rawUrl:?}");
		}
	}
}
