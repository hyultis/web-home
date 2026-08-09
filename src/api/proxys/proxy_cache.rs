use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

pub(crate) const CACHE_DIR: &str = "./dynamic/proxy_cache";

static CACHE_FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone)]
pub(super) struct ProxyCache
{
	path: PathBuf,
}

impl ProxyCache
{
	pub(super) fn get(cacheType: impl AsRef<str>) -> Result<Self, io::Error>
	{
		let cacheType = cacheType.as_ref();
		if (!Self::pathSegment_isValid(cacheType))
		{
			return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid proxy cache type"));
		}
		let path = Path::new(CACHE_DIR).join(cacheType);
		fs::create_dir_all(&path)?;
		return Ok(Self { path });
	}

	pub(super) fn load(&self, contentHash: impl AsRef<str>, maximumBytes: usize) -> Result<Option<Vec<u8>>, io::Error>
	{
		let _lock = CACHE_FILE_LOCK.lock().map_err(|_| io::Error::other("proxy cache lock poisoned"))?;
		return self.entry_get(contentHash)?.content_get(maximumBytes);
	}

	pub(super) fn save(&self, contentHash: impl AsRef<str>, content: &[u8]) -> Result<(), io::Error>
	{
		let _lock = CACHE_FILE_LOCK.lock().map_err(|_| io::Error::other("proxy cache lock poisoned"))?;
		return self.entry_get(contentHash)?.content_save(content);
	}

	pub(super) fn remove(&self, contentHash: impl AsRef<str>) -> Result<(), io::Error>
	{
		let _lock = CACHE_FILE_LOCK.lock().map_err(|_| io::Error::other("proxy cache lock poisoned"))?;
		let entry = self.entry_get(contentHash)?;
		return match fs::remove_file(entry.path)
		{
			Ok(()) => Ok(()),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
			Err(error) => Err(error),
		};
	}

	pub(super) fn cleanup(&self, maximumBytes: u64, maximumEntries: usize, maximumAge: Duration) -> Result<(), io::Error>
	{
		let _lock = CACHE_FILE_LOCK.lock().map_err(|_| io::Error::other("proxy cache lock poisoned"))?;
		let now = SystemTime::now();
		let mut entries = Vec::new();
		for entry in fs::read_dir(&self.path)?
		{
			let entry = entry?;
			if (!entry.file_type()?.is_file())
			{
				continue;
			}
			let metadata = entry.metadata()?;
			let modifiedAt = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
			let age = now.duration_since(modifiedAt).unwrap_or(Duration::ZERO);
			let fileName = entry.file_name().to_string_lossy().to_string();
			if (fileName.starts_with('.') && fileName.ends_with(".tmp"))
			{
				if (age > Duration::from_secs(60 * 60))
				{
					fs::remove_file(entry.path())?;
				}
				continue;
			}
			if (age > maximumAge)
			{
				fs::remove_file(entry.path())?;
				continue;
			}
			entries.push(ProxyCacheFile {
				modifiedAt,
				path: entry.path(),
				size: metadata.len(),
			});
		}
		entries.sort_unstable_by(|left, right| right.modifiedAt.cmp(&left.modifiedAt));

		let mut retainedBytes = 0u64;
		for (index, entry) in entries.into_iter().enumerate()
		{
			let nextSize = retainedBytes.saturating_add(entry.size);
			if (index >= maximumEntries || nextSize > maximumBytes)
			{
				fs::remove_file(entry.path)?;
				continue;
			}
			retainedBytes = nextSize;
		}
		return Ok(());
	}

	fn entry_get(&self, contentHash: impl AsRef<str>) -> Result<ProxyCacheEntry, io::Error>
	{
		let contentHash = contentHash.as_ref().replace('/', "LL");
		if (!Self::pathSegment_isValid(&contentHash))
		{
			return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid proxy cache key"));
		}
		return Ok(ProxyCacheEntry { path: self.path.join(contentHash) });
	}

	fn pathSegment_isValid(value: &str) -> bool
	{
		return !value.is_empty() && value.chars().all(|character| {
			return character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_' | '=');
		});
	}

	#[cfg(test)]
	pub(super) fn test_get(path: PathBuf) -> Result<Self, io::Error>
	{
		fs::create_dir_all(&path)?;
		return Ok(Self { path });
	}
}

struct ProxyCacheEntry
{
	path: PathBuf,
}

impl ProxyCacheEntry
{
	fn content_get(&self, maximumBytes: usize) -> Result<Option<Vec<u8>>, io::Error>
	{
		let mut file = match File::open(&self.path)
		{
			Ok(file) => file,
			Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(error) => return Err(error),
		};
		let maximumRead = u64::try_from(maximumBytes).unwrap_or(u64::MAX).saturating_add(1);
		let mut content = Vec::with_capacity(maximumBytes.min(64 * 1024));
		Read::by_ref(&mut file).take(maximumRead).read_to_end(&mut content)?;
		if (content.len() > maximumBytes)
		{
			return Err(io::Error::new(io::ErrorKind::InvalidData, "proxy cache entry exceeds its size limit"));
		}
		return Ok(Some(content));
	}

	fn content_save(&self, content: &[u8]) -> Result<(), io::Error>
	{
		let fileName = self.path.file_name()
			.and_then(|name| name.to_str())
			.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid proxy cache file name"))?;
		let temporaryPath = self.path.with_file_name(format!(".{}.{}.tmp", fileName, uuid::Uuid::new_v4()));
		let result = (|| -> Result<(), io::Error>
		{
			let mut file = OpenOptions::new().create_new(true).write(true).open(&temporaryPath)?;
			file.write_all(content)?;
			file.sync_all()?;
			fs::rename(&temporaryPath, &self.path)?;
			return Ok(());
		})();
		if (result.is_err())
		{
			let _ = fs::remove_file(&temporaryPath);
		}
		return result;
	}
}

struct ProxyCacheFile
{
	modifiedAt: SystemTime,
	path: PathBuf,
	size: u64,
}

#[cfg(test)]
mod tests
{
	use super::*;

	struct TestCache
	{
		cache: ProxyCache,
		root: PathBuf,
	}

	impl TestCache
	{
		fn new() -> Self
		{
			let root = std::env::temp_dir().join(format!("webhome-proxy-cache-test-{}", uuid::Uuid::new_v4()));
			let path = root.join("wget");
			fs::create_dir_all(&path).unwrap();
			return Self { cache: ProxyCache { path }, root };
		}
	}

	impl Drop for TestCache
	{
		fn drop(&mut self)
		{
			let _ = fs::remove_dir_all(&self.root);
		}
	}

	#[test]
	fn cacheEntry_saveAndBoundedLoad_areCoherent()
	{
		let cache = TestCache::new();
		cache.cache.save("valid-key=", b"content").unwrap();
		assert_eq!(cache.cache.load("valid-key=", 7).unwrap(), Some(b"content".to_vec()));
		assert_eq!(cache.cache.load("valid-key=", 6).unwrap_err().kind(), io::ErrorKind::InvalidData);
		assert!(fs::read_dir(cache.cache.path.clone()).unwrap().all(|entry| {
			return !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp");
		}));
	}

	#[test]
	fn cacheCleanup_limitsEntryCountAndTotalSize()
	{
		let cache = TestCache::new();
		cache.cache.save("entry-a", b"aaaa").unwrap();
		cache.cache.save("entry-b", b"bbbb").unwrap();
		cache.cache.cleanup(4, 1, Duration::from_secs(60)).unwrap();
		let retained = fs::read_dir(&cache.cache.path).unwrap().count();
		assert_eq!(retained, 1);
	}

	#[test]
	fn cachePath_rejectsTraversalSegments()
	{
		let cache = TestCache::new();
		assert_eq!(cache.cache.load("../outside", 10).unwrap_err().kind(), io::ErrorKind::InvalidInput);
	}
}
