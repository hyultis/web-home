use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use Hconfig::HConfigManager::HConfigManager;
use Hconfig::tinyjson::JsonValue;
use async_lock::{Semaphore, SemaphoreGuard};
use imap::Connection;
use native_tls::TlsConnector;
use reqwest::redirect::Policy;
use reqwest::Client;
use url::{Host, Url};

use crate::api::login::user_back::{AuthenticatedUser, UserBackHelperError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OutboundPolicyError
{
	AuthenticationRequired,
	ConfigurationInvalid,
	DestinationForbidden,
	Internal,
	ResolutionFailed,
	ResourceLimitReached,
}

struct OutboundLimits;

impl OutboundLimits
{
	const DNS_TIMEOUT: Duration = Duration::from_secs(5);
	const DNS_ADDRESS_MAX: usize = 16;
	const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
	const HTTP_CONCURRENCY_MAX: usize = 8;
	const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
	const HTTP_REDIRECT_MAX: usize = 5;
	const IMAP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
	const IMAP_CONCURRENCY_MAX: usize = 4;
	const IMAP_IO_TIMEOUT: Duration = Duration::from_secs(30);
}

static HTTP_CONCURRENCY: Semaphore = Semaphore::new(OutboundLimits::HTTP_CONCURRENCY_MAX);
static IMAP_CONCURRENCY: Semaphore = Semaphore::new(OutboundLimits::IMAP_CONCURRENCY_MAX);

#[derive(Debug)]
struct ImapAllowedPorts(Vec<u16>);

impl ImapAllowedPorts
{
	const CONFIG_KEY: &'static str = "imap_allowed_ports";
	const DEFAULT_PORT: u16 = 993;

	fn current_get() -> Result<Self, OutboundPolicyError>
	{
		let Some(siteConfig) = HConfigManager::singleton().get("site")
		else
		{
			return Ok(Self::default());
		};
		return Self::fromConfigValue(siteConfig.value_get(Self::CONFIG_KEY));
	}

	fn fromConfigValue(value: Option<JsonValue>) -> Result<Self, OutboundPolicyError>
	{
		let Some(value) = value
		else
		{
			return Ok(Self::default());
		};
		let JsonValue::Array(values) = value
		else
		{
			return Err(OutboundPolicyError::ConfigurationInvalid);
		};
		let mut ports = Vec::with_capacity(values.len());
		for value in values
		{
			let JsonValue::Number(port) = value
			else
			{
				return Err(OutboundPolicyError::ConfigurationInvalid);
			};
			if (!port.is_finite() || port.fract() != 0.0 || port < 1.0 || port > u16::MAX as f64)
			{
				return Err(OutboundPolicyError::ConfigurationInvalid);
			}
			ports.push(port as u16);
		}
		ports.sort_unstable();
		ports.dedup();
		if (ports.is_empty())
		{
			return Err(OutboundPolicyError::ConfigurationInvalid);
		}
		return Ok(Self(ports));
	}

	fn contains(&self, port: u16) -> bool
	{
		return self.0.binary_search(&port).is_ok();
	}
}

impl Default for ImapAllowedPorts
{
	fn default() -> Self
	{
		return Self(vec![Self::DEFAULT_PORT]);
	}
}

#[derive(Clone, Debug)]
pub(super) struct ValidatedHttpDestination
{
	addresses: Vec<SocketAddr>,
	host: String,
	url: Url,
}

impl ValidatedHttpDestination
{
	pub(super) fn client_get(&self) -> Result<Client, OutboundPolicyError>
	{
		return Client::builder()
			.connect_timeout(OutboundLimits::HTTP_CONNECT_TIMEOUT)
			.timeout(OutboundLimits::HTTP_REQUEST_TIMEOUT)
			.redirect(Policy::none())
			.no_proxy()
			.resolve_to_addrs(&self.host, &self.addresses)
			.build()
			.map_err(|_| OutboundPolicyError::Internal);
	}

	pub(super) fn url_get(&self) -> &Url
	{
		return &self.url;
	}

	pub(super) async fn redirected_get(&self, location: &str) -> Result<Self, OutboundPolicyError>
	{
		let redirectedUrl = self.url.join(location).map_err(|_| OutboundPolicyError::DestinationForbidden)?;
		return OutboundPolicy::httpDestination_get(redirectedUrl.as_str()).await;
	}

	pub(super) fn redirectMaximum_get() -> usize
	{
		return OutboundLimits::HTTP_REDIRECT_MAX;
	}

	#[cfg(test)]
	pub(super) fn test_get(url: Url, addresses: Vec<SocketAddr>) -> Self
	{
		let host = url.host_str().unwrap_or_default().to_string();
		return Self { addresses, host, url };
	}
}

pub(super) struct ValidatedImapDestination
{
	addresses: Vec<SocketAddr>,
	host: String,
}

impl ValidatedImapDestination
{
	pub(super) fn connection_get(&self) -> Result<Connection, imap::Error>
	{
		let deadline = Instant::now() + OutboundLimits::IMAP_CONNECT_TIMEOUT;
		let mut lastError = None;
		for address in &self.addresses
		{
			let remaining = deadline.saturating_duration_since(Instant::now());
			if (remaining.is_zero())
			{
				break;
			}
			match TcpStream::connect_timeout(address, remaining)
			{
				Ok(stream) =>
				{
					stream.set_read_timeout(Some(OutboundLimits::IMAP_IO_TIMEOUT))?;
					stream.set_write_timeout(Some(OutboundLimits::IMAP_IO_TIMEOUT))?;
					let connector = TlsConnector::builder().build()?;
					let stream = connector.connect(&self.host, stream)?;
					return Ok(Box::new(stream));
				},
				Err(error) => lastError = Some(error),
			}
		}
		return Err(lastError.unwrap_or_else(||
		{
			return std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no validated IMAP address");
		}).into());
	}

	#[cfg(test)]
	pub(super) fn test_get(host: String, addresses: Vec<SocketAddr>) -> Self
	{
		return Self { addresses, host };
	}
}

pub(super) struct OutboundPolicy;

impl OutboundPolicy
{
	pub(super) fn httpPermit_get() -> Result<SemaphoreGuard<'static>, OutboundPolicyError>
	{
		return HTTP_CONCURRENCY.try_acquire().ok_or(OutboundPolicyError::ResourceLimitReached);
	}

	pub(super) fn imapPermit_get() -> Result<SemaphoreGuard<'static>, OutboundPolicyError>
	{
		return IMAP_CONCURRENCY.try_acquire().ok_or(OutboundPolicyError::ResourceLimitReached);
	}

	pub(super) async fn authentication_require() -> Result<(), OutboundPolicyError>
	{
		return AuthenticatedUser::current().await
			.map(|_| ())
			.map_err(|error| match error
			{
				UserBackHelperError::LoginError(_) => OutboundPolicyError::AuthenticationRequired,
				_ => OutboundPolicyError::Internal,
			});
	}

	pub(super) async fn httpDestination_get(rawUrl: &str) -> Result<ValidatedHttpDestination, OutboundPolicyError>
	{
		if (rawUrl.len() > 8 * 1024)
		{
			return Err(OutboundPolicyError::DestinationForbidden);
		}
		let url = Url::parse(rawUrl).map_err(|_| OutboundPolicyError::DestinationForbidden)?;
		if (!matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some())
		{
			return Err(OutboundPolicyError::DestinationForbidden);
		}
		let host = url.host().ok_or(OutboundPolicyError::DestinationForbidden)?;
		let port = url.port_or_known_default().ok_or(OutboundPolicyError::DestinationForbidden)?;
		let host = match host
		{
			Host::Domain(host) => host.to_string(),
			Host::Ipv4(host) => host.to_string(),
			Host::Ipv6(host) => host.to_string(),
		};
		let addresses = Self::addresses_get(&host, port).await?;
		return Ok(ValidatedHttpDestination { addresses, host, url });
	}

	pub(super) async fn imapDestination_get(host: &str, port: u16) -> Result<ValidatedImapDestination, OutboundPolicyError>
	{
		if (host.is_empty() || host.len() > 253 || host.trim() != host || host.contains('/') || host.contains('@'))
		{
			return Err(OutboundPolicyError::DestinationForbidden);
		}
		if (!ImapAllowedPorts::current_get()?.contains(port))
		{
			return Err(OutboundPolicyError::DestinationForbidden);
		}
		let addresses = Self::addresses_get(host, port).await?;
		return Ok(ValidatedImapDestination { addresses, host: host.to_string() });
	}

	async fn addresses_get(host: &str, port: u16) -> Result<Vec<SocketAddr>, OutboundPolicyError>
	{
		let mut addresses = if let Ok(address) = host.parse::<IpAddr>()
		{
			vec![SocketAddr::new(address, port)]
		}
		else
		{
			let resolved = tokio::time::timeout(
				OutboundLimits::DNS_TIMEOUT,
				tokio::net::lookup_host((host, port)),
			).await.map_err(|_| OutboundPolicyError::ResolutionFailed)?
				.map_err(|_| OutboundPolicyError::ResolutionFailed)?;
			resolved.collect::<Vec<_>>()
		};
		addresses.sort_unstable();
		addresses.dedup();
		if (addresses.is_empty() || addresses.len() > OutboundLimits::DNS_ADDRESS_MAX)
		{
			return Err(OutboundPolicyError::ResolutionFailed);
		}
		if (addresses.iter().any(|address| !Self::address_isPublic(address.ip())))
		{
			return Err(OutboundPolicyError::DestinationForbidden);
		}
		return Ok(addresses);
	}

	fn address_isPublic(address: IpAddr) -> bool
	{
		return match address
		{
			IpAddr::V4(address) => Self::ipv4_isPublic(address),
			IpAddr::V6(address) => Self::ipv6_isPublic(address),
		};
	}

	fn ipv4_isPublic(address: Ipv4Addr) -> bool
	{
		let [first, second, third, fourth] = address.octets();
		return !(first == 0
			|| address.is_private()
			|| address.is_loopback()
			|| address.is_link_local()
			|| address.is_multicast()
			|| address.is_documentation()
			|| (first == 100 && (second & 0b1100_0000) == 0b0100_0000)
			|| (first == 192 && second == 0 && third == 0)
			|| (first == 192 && second == 88 && third == 99)
			|| (first == 198 && (second & 0xfe) == 18)
			|| first >= 240
			|| [first, second, third, fourth] == [255, 255, 255, 255]);
	}

	fn ipv6_isPublic(address: Ipv6Addr) -> bool
	{
		let segments = address.segments();
		return (segments[0] & 0xe000) == 0x2000
			&& !matches!(segments, [0x2001, 0x0000..=0x01ff, ..])
			&& !matches!(segments, [0x2001, 0x0db8, ..])
			&& !matches!(segments, [0x2002, ..])
			&& !matches!(segments, [0x3fff, 0x0000..=0x0fff, ..]);
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn imapAllowedPorts_defaultsToTlsPort()
	{
		let ports = ImapAllowedPorts::fromConfigValue(None).unwrap();
		assert!(ports.contains(993));
		assert!(!ports.contains(143));
	}

	#[test]
	fn imapAllowedPorts_requiresNonEmptyIntegerArray()
	{
		assert_eq!(
			ImapAllowedPorts::fromConfigValue(Some(JsonValue::Array(vec![]))).unwrap_err(),
			OutboundPolicyError::ConfigurationInvalid,
		);
		assert_eq!(
			ImapAllowedPorts::fromConfigValue(Some(JsonValue::Array(vec![993.5.into()]))).unwrap_err(),
			OutboundPolicyError::ConfigurationInvalid,
		);
		assert_eq!(
			ImapAllowedPorts::fromConfigValue(Some(JsonValue::String("993".to_string()))).unwrap_err(),
			OutboundPolicyError::ConfigurationInvalid,
		);
	}

	#[test]
	fn imapAllowedPorts_acceptsAndDeduplicatesConfiguredPorts()
	{
		let ports = ImapAllowedPorts::fromConfigValue(Some(JsonValue::Array(vec![
			993.0.into(),
			1993.0.into(),
			993.0.into(),
		]))).unwrap();
		assert!(ports.contains(993));
		assert!(ports.contains(1993));
		assert_eq!(ports.0.len(), 2);
	}

	#[test]
	fn publicAddressPolicy_rejectsSpecialIpv4Ranges()
	{
		for address in [
			"0.0.0.0",
			"10.0.0.1",
			"100.64.0.1",
			"127.0.0.1",
			"169.254.169.254",
			"172.16.0.1",
			"192.0.2.1",
			"192.168.0.1",
			"198.18.0.1",
			"224.0.0.1",
			"255.255.255.255",
		]
		{
			assert!(!OutboundPolicy::address_isPublic(address.parse().unwrap()), "{}", address);
		}
		assert!(OutboundPolicy::address_isPublic("8.8.8.8".parse().unwrap()));
	}

	#[test]
	fn publicAddressPolicy_rejectsSpecialIpv6Ranges()
	{
		for address in [
			"::",
			"::1",
			"::ffff:127.0.0.1",
			"fc00::1",
			"fe80::1",
			"2001:db8::1",
			"2002::1",
			"3fff::1",
			"ff02::1",
		]
		{
			assert!(!OutboundPolicy::address_isPublic(address.parse().unwrap()), "{}", address);
		}
		assert!(OutboundPolicy::address_isPublic("2606:4700:4700::1111".parse().unwrap()));
	}

	#[test]
	fn httpDestination_rejectsLocalAndNonHttpUrls()
	{
		let runtime = tokio::runtime::Runtime::new().unwrap();
		runtime.block_on(async {
			assert_eq!(
				OutboundPolicy::httpDestination_get("http://127.0.0.1/feed").await.unwrap_err(),
				OutboundPolicyError::DestinationForbidden,
			);
			assert_eq!(
				OutboundPolicy::httpDestination_get("file:///etc/passwd").await.unwrap_err(),
				OutboundPolicyError::DestinationForbidden,
			);
			assert_eq!(
				OutboundPolicy::httpDestination_get("https://user:secret@8.8.8.8/feed").await.unwrap_err(),
				OutboundPolicyError::DestinationForbidden,
			);
			for url in [
				"http://2130706433/feed",
				"http://0x7f000001/feed",
				"http://[::ffff:127.0.0.1]/feed",
				"http://localhost/feed",
			]
			{
				assert_eq!(
					OutboundPolicy::httpDestination_get(url).await.unwrap_err(),
					OutboundPolicyError::DestinationForbidden,
					"{}",
					url,
				);
			}

			let destination = OutboundPolicy::httpDestination_get("https://8.8.8.8/feed").await.unwrap();
			assert_eq!(
				destination.redirected_get("http://127.0.0.1/private").await.unwrap_err(),
				OutboundPolicyError::DestinationForbidden,
			);
			assert_eq!(
				destination.redirected_get("/next").await.unwrap().url_get().as_str(),
				"https://8.8.8.8/next",
			);
		});
	}
}
