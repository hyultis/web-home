use leptos::server;
use leptos::server_fn::codec::Json;

use crate::api::proxys::imap_components::{BoxName, ImapMailboxSync, ImapMail, ImapMailKey, ImapSyncRequest, imap_connector};
use crate::api::proxys::imap_error::ImapError;

#[server]
pub async fn API_proxys_imap_listbox(config: imap_connector) -> Result<Vec<BoxName>, ImapError>
{
	use crate::api::proxys::imap_inner::ImapProxy;

	return inner::ImapRequest::new(config,"list").await?
		.run(ImapProxy::listbox_get).await;
}

#[server(input = Json)]
pub async fn API_proxys_imap_sync(config: imap_connector, request: ImapSyncRequest) -> Result<Vec<ImapMailboxSync>, ImapError>
{
	#[cfg(feature = "ssr")]
	{
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		let mailboxCount = request.mailboxes.as_ref().map_or(0,Vec::len);
		let knownUidCount = request.mailboxes.as_ref().map(|mailboxes| {
			return mailboxes.iter().map(|mailbox| mailbox.knownUids.len()).sum::<usize>();
		}).unwrap_or(0);
		HTrace!(
			(Level::DEBUG)
			"[IMAP proxy] operation=sync stage=entry mailbox_count={} known_uid_count={}",
			mailboxCount,
			knownUidCount
		);
	}
	let result = inner::ImapRequest::new(config,"sync").await?
		.run(move |proxy| proxy.sync_get(request)).await;
	#[cfg(feature = "ssr")]
	if let Ok(mailboxes) = &result
	{
		use Htrace::components::level::Level;
		use Htrace::HTrace;

		let mailCount = mailboxes.iter().map(|mailbox| mailbox.mails.len()).sum::<usize>();
		HTrace!(
			(Level::DEBUG)
			"[IMAP proxy] operation=sync stage=success mailbox_count={} mail_count={}",
			mailboxes.len(),
			mailCount
		);
	}
	return result;
}

#[server]
pub async fn API_proxys_imap_getMailContent(config: imap_connector, mail: ImapMailKey) -> Result<ImapMail, ImapError>
{
	return inner::ImapRequest::new(config,"content").await?
		.run(move |proxy| proxy.mailContent_get(mail)).await;
}

#[server]
pub async fn API_proxys_imap_getMailAiContent(config: imap_connector, mail: ImapMailKey) -> Result<String, ImapError>
{
	return inner::ImapRequest::new(config,"ai_content").await?
		.run(move |proxy| proxy.mailAiContent_get(mail)).await;
}

#[server]
pub async fn API_proxys_imap_setMailSee(config: imap_connector, mail: ImapMailKey) -> Result<(), ImapError>
{
	return inner::ImapRequest::new(config,"seen").await?
		.run(move |proxy| proxy.mailSeen_set(mail)).await;
}

#[cfg(test)]
mod contract_tests
{
	use leptos::server_fn::{ContentType, Http, ServerFn};
	use leptos::server_fn::codec::Encoding;

	use super::ApiProxysImapSync;

	trait HttpInput
	{
		type Encoding;
	}

	impl<Input,Output> HttpInput for Http<Input,Output>
	{
		type Encoding = Input;
	}

	#[test]
	fn syncUsesJsonRequestBody()
	{
		type SyncInput = <<ApiProxysImapSync as ServerFn>::Protocol as HttpInput>::Encoding;

		assert_eq!(<SyncInput as ContentType>::CONTENT_TYPE,"application/json");
		assert_eq!(<SyncInput as Encoding>::METHOD,http::Method::POST);
	}
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
		operation: &'static str,
		permit: SemaphoreGuard<'static>,
		proxy: ImapProxy,
	}

	impl ImapRequest
	{
		pub(super) async fn new(config: imap_connector, operation: &'static str) -> Result<Self, ImapError>
		{
			OutboundPolicy::authentication_require().await
				.map_err(|error| ImapError::from(error).trace(operation,"authentication",None))?;
			let permit = OutboundPolicy::imapPermit_get()
				.map_err(|error| ImapError::from(error).trace(operation,"concurrency",None))?;
			let destination = OutboundPolicy::imapDestination_get(&config.host, config.port).await
				.map_err(|error| ImapError::from(error).trace(operation,"destination",None))?;
			let proxy = ImapProxy::new(config, destination)
				.map_err(|error| error.trace(operation,"config",None))?;
			return Ok(Self { operation, permit, proxy });
		}

		pub(super) async fn run<Output, Operation>(self, operationFn: Operation) -> Result<Output, ImapError>
		where
			Output: Send + 'static,
			Operation: FnOnce(ImapProxy) -> Result<Output, ImapError> + Send + 'static,
		{
			let Self { operation, permit, proxy } = self;
			let result = tokio::task::spawn_blocking(move ||
			{
				let _permit = permit;
				return operationFn(proxy);
			}).await.map_err(|error| ImapError::from(error).trace(operation,"worker",None))?;
			return result.map_err(|error| error.trace(operation,"complete",None));
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
				let workerThread = ImapRequest { operation: "test", permit, proxy }
					.run(|_| Ok(thread::current().id())).await.unwrap();
				assert_ne!(workerThread, callerThread);
			});
		}
	}
}
