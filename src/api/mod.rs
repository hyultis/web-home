use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use leptoaster::ToastLevel;

pub mod translateBooks;
pub mod Htrace;
pub mod login;
pub mod modules;
pub mod proxys;

pub trait IsToastable: ToString {
	// None if nothing is toasted, otherwise return the level of the toast
	fn level(&self) -> Option<ToastLevel>;
	fn authenticationRequired_get(&self) -> bool;
}

pub static IS_TRACE_FRONT_LOG: OnceLock<AtomicBool> = OnceLock::new();
pub static ALLOW_REGISTRATION: OnceLock<AtomicBool> = OnceLock::new();

pub(crate) fn runtimeConfig_set(traceFrontLog: bool,allowRegistration: bool)
{
	let _ = IS_TRACE_FRONT_LOG.set(AtomicBool::new(traceFrontLog));
	let _ = ALLOW_REGISTRATION.set(AtomicBool::new(allowRegistration));
}
