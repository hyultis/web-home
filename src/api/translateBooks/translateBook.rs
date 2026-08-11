use std::fs;
use std::io::Read;
use std::time::UNIX_EPOCH;
use anyhow::anyhow;
use Htrace::HTraceError;

#[derive(Debug)]
pub(super) struct TranslateBook {
	_timestamp: u64,
	_content: String
}

impl TranslateBook {
	pub(super) fn get(&self) -> (String,u64)
	{
		return (self._content.clone(),self._timestamp);
	}

	#[cfg(feature = "ssr")]
	pub(super) fn load(lang: &str) -> anyhow::Result<TranslateBook>
	{
		let path = format!("./static/translates/{lang}/main.flt");
		let mut ftl_content_file = match fs::File::open(&path)
		{
			Ok(file) => file,
			Err(_) => {
				if(lang == "EN")
				{
					return Err(anyhow!("unable to load EN fluent file."));
				}
				return Ok(Self::load("EN")?);
			}
		};

		let mut ftl_content= "".to_string();
		HTraceError!(ftl_content_file.read_to_string(&mut ftl_content));

		let metadata = ftl_content_file.metadata()?;
		let modified = metadata.modified()?.duration_since(UNIX_EPOCH)
			.map_err(|_| anyhow!("invalid fluent file modification time"))?;

		return Ok(Self {
			_timestamp: modified.as_millis() as u64,
			_content: ftl_content
		});
	}
}
