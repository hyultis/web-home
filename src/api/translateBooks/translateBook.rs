use std::fs;
use std::io::Read;
use std::time::UNIX_EPOCH;
use anyhow::anyhow;
use Htrace::HTraceError;

#[derive(Debug)]
pub struct TranslateBook {
	_timestamp: u64,
	_content: String
}

impl TranslateBook {
	pub fn new() -> Self {
		Self {
			_timestamp: 0,
			_content: "".to_string()
		}
	}

	pub fn get(&self) -> (String,u64)
	{
		return (self._content.clone(),self._timestamp);
	}

	pub fn getTime(&self) -> u64
	{
		return self._timestamp;
	}

	#[cfg(feature = "ssr")]
	pub fn load(lang: &String) -> anyhow::Result<TranslateBook>
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
				return Ok(Self::load(&"EN".to_string())?);
			}
		};

		let mut ftl_content= "".to_string();
		HTraceError!(ftl_content_file.read_to_string(&mut ftl_content));

		let metadata = ftl_content_file.metadata().unwrap();

		return Ok(Self {
			_timestamp: metadata.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
			_content: ftl_content
		});
	}
}