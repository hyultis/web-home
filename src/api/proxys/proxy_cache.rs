use std::{fs, io};
use std::fs::File;
use std::path::Path;
use std::time::SystemTime;

pub const CACHE_DIR: &str = "./dynamic/proxy_cache";

pub struct ProxyCache
{
	cachetype: String,
}

impl ProxyCache
{
	pub fn get(cachetype: impl ToString) -> Result<Self, io::Error>
	{
		let cachetype = cachetype.to_string();

		let path = format!("{}/{}",CACHE_DIR, cachetype);
		if(!Path::new(&path).exists()) {
			fs::create_dir(&path)?;
		}

		Ok(Self{ cachetype })
	}

	pub fn content_exists(&self, contentHash: impl AsRef<str>) -> bool
	{
		let contentHash = contentHash.as_ref().replace("/", "LL");
		fs::metadata(format!("{}/{}/{}",CACHE_DIR,self.cachetype,contentHash)).is_ok()
	}

	pub fn content_updateTime(&self,contentHash: impl AsRef<str>, newTime: SystemTime)
	{
		let contentHash = contentHash.as_ref().replace("/", "LL");

		let Ok(file) = File::create(format!("{}/{}/{}",CACHE_DIR,self.cachetype,contentHash)) else {return;};
		file.set_modified(newTime).unwrap();
	}

	pub fn content_lastUpdate(&self,contentHash: impl AsRef<str>) -> Option<u64>
	{
		let contentHash = contentHash.as_ref().replace("/", "LL");
		let Ok(metadata) = fs::metadata(format!("{}/{}/{}",CACHE_DIR,self.cachetype,contentHash)) else {return None;};
		let Ok(lastUpdate) = metadata.modified() else {return None;};

		let Ok(duration) = lastUpdate.duration_since(std::time::UNIX_EPOCH) else {return None;};
		return Some(duration.as_secs());
	}

	pub fn load(&self,contentHash: impl AsRef<str>) -> Option<String>
	{
		let contentHash = contentHash.as_ref().replace("/", "LL");
		let Ok(raw) = fs::read(format!("{}/{}/{}",CACHE_DIR,self.cachetype,contentHash)) else {return None;};
		return Some(String::from_utf8(raw).unwrap());
	}

	pub fn save(&self,contentHash: impl AsRef<str>, content: impl ToString) -> bool
	{
		let contentHash = contentHash.as_ref().replace("/", "LL");
		return fs::write(format!("{}/{}/{}",CACHE_DIR,self.cachetype,contentHash),content.to_string()).is_ok();
	}
}