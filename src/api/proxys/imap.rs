use std::collections::HashMap;

use leptos::server;

use crate::api::proxys::imap_components::{BoxName, ImapMail, ImapMailIdentifier, ImapMailUpdate, imap_connector};
use crate::api::proxys::imap_error::ImapError;

#[server]
pub async fn API_proxys_imap_listbox(config: imap_connector) -> Result<Vec<BoxName>, ImapError>
{
	use crate::api::proxys::imap_inner::ImapProxy;

	return inner::ImapRequest::new(config).await?
		.run(ImapProxy::listbox_get).await;
}

#[server]
pub async fn API_proxys_imap_getFullUnsee(config: imap_connector) -> Result<Vec<ImapMail>, ImapError>
{
	use crate::api::proxys::imap_inner::ImapProxy;

	return inner::ImapRequest::new(config).await?
		.run(ImapProxy::fullUnseen_get).await;
}

#[server]
pub async fn API_proxys_imap_getUnseeSince(
	config: imap_connector,
	date: u64,
	toUpdate: Vec<u32>,
) -> Result<(Vec<ImapMail>,HashMap<u32,ImapMailUpdate>), ImapError>
{
	return inner::ImapRequest::new(config).await?
		.run(move |proxy| proxy.unseenSince_get(date, toUpdate)).await;
}

#[server]
pub async fn API_proxys_imap_getMailContent(config: imap_connector, mail: ImapMailIdentifier) -> Result<ImapMail, ImapError>
{
	return inner::ImapRequest::new(config).await?
		.run(move |proxy| proxy.mailContent_get(mail)).await;
}

#[server]
pub async fn API_proxys_imap_setMailSee(config: imap_connector, mail: ImapMailIdentifier) -> Result<(), ImapError>
{
	return inner::ImapRequest::new(config).await?
		.run(move |proxy| proxy.mailSeen_set(mail)).await;
}

#[cfg(feature = "ssr")]
mod inner
{
	use async_lock::SemaphoreGuard;

	use crate::api::proxys::imap_components::imap_connector;
	use crate::api::proxys::imap_error::ImapError;
	use crate::api::proxys::imap_inner::ImapProxy;
	use crate::api::proxys::outbound_policy::OutboundPolicy;

	pub(super) struct ImapRequest
	{
		permit: SemaphoreGuard<'static>,
		proxy: ImapProxy,
	}

	impl ImapRequest
	{
		pub(super) async fn new(config: imap_connector) -> Result<Self, ImapError>
		{
			OutboundPolicy::authentication_require().await.map_err(ImapError::from)?;
			let permit = OutboundPolicy::imapPermit_get().map_err(ImapError::from)?;
			let destination = OutboundPolicy::imapDestination_get(&config.host, config.port).await
				.map_err(ImapError::from)?;
			return Ok(Self { permit, proxy: ImapProxy::new(config, destination)? });
		}

		pub(super) async fn run<Output, Operation>(self, operation: Operation) -> Result<Output, ImapError>
		where
			Output: Send + 'static,
			Operation: FnOnce(ImapProxy) -> Result<Output, ImapError> + Send + 'static,
		{
			let Self { permit, proxy } = self;
			let result = tokio::task::spawn_blocking(move ||
			{
				let _permit = permit;
				return operation(proxy);
			}).await
				.map_err(ImapError::from)?;
			return result;
		}
	}

	#[cfg(test)]
	mod tests
	{
		use std::thread;

		use crate::api::proxys::imap_components::imap_connector;
		use crate::api::proxys::imap_inner::ImapProxy;
		use crate::api::proxys::outbound_policy::{OutboundPolicy, ValidatedImapDestination};

		use super::ImapRequest;

		#[test]
		fn request_runsOperationOnBlockingWorker()
		{
			let runtime = tokio::runtime::Runtime::new().unwrap();
			runtime.block_on(async {
				let callerThread = thread::current().id();
				let permit = OutboundPolicy::imapPermit_get().unwrap();
				let destination = ValidatedImapDestination::test_get(
					"imap.example.com".to_string(),
					vec!["8.8.8.8:993".parse().unwrap()],
				);
				let proxy = ImapProxy::new(imap_connector::default(), destination).unwrap();
				let workerThread = ImapRequest { permit, proxy }
					.run(|_| Ok(thread::current().id())).await.unwrap();
				assert_ne!(workerThread, callerThread);
			});
		}
	}
}
