#![allow(unused_imports)]

use std::any::Any;
use std::fmt::{Debug, Display};
use leptos::logging::log;
use leptos::prelude::{use_context, Action};
use crate::api::Htrace::{API_Htrace_log, Type};

pub fn action_to_trace<T>(rawEntry: &T, htype: Type, file: &str, line: u32)
	where T: Any + Debug // + ?Display
{

	let anyEntry = rawEntry as &dyn Any;
	let tmp = if let Some(content) = anyEntry.downcast_ref::<String>() {
		content.to_string()
	}
	else
	{
		if let Some(content) = anyEntry.downcast_ref::<&str>()
		{
			content.to_string()
		}
		else
		{
			if let Some(content)= anyEntry.downcast_ref::<Box<dyn Display>>()
			{
				format!("{}", content)
			}
			else
			{
				format!("{:?}", rawEntry)
			}
		}
	};

	let file = file.to_string();

	let traceFrontLog = crate::api::IS_TRACE_FRONT_LOG.get().map(|ab| ab.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
	if(!traceFrontLog) {
		log!("{:?} ({}:{}) : {}", htype,file, line,tmp);
		return;
	}

	let tmptrace = Action::new(move |_|{
		let tmp = tmp.clone();
		let file = file.clone();
		let htype = htype.clone();
		async move { let _ = API_Htrace_log(tmp.clone(), htype, file, line).await; }
	});

	tmptrace.dispatch(());
}

// example <li on:click=move |_| { HWebTrace!((Type::ERROR) "type"); }>test</li>
// no backtrace for the moment
#[macro_export]
macro_rules! HWebTrace {
	($a:expr) => {
	    $crate::front::utils::trace::action_to_trace(&$a, $crate::api::Htrace::Type::NORMAL, file!(), line!())
    };
	(($b:expr) $a:expr) => {
		$crate::front::utils::trace::action_to_trace(&$a, $b, file!(), line!());
    };
	($a:expr $(,$arg:expr)*) => {
	    $crate::front::utils::trace::action_to_trace(&format!($a,$($arg),*), $crate::api::Htrace::Type::NORMAL, file!(), line!())
    };
	(($b:expr) $a:expr $(,$arg:expr)*) => {
		$crate::front::utils::trace::action_to_trace(&format!($a,$($arg),*), $b, file!(), line!())
    };
}

pub(crate) use HWebTrace;
