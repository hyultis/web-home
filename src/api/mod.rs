use std::time::Duration;

pub mod translateBooks;
pub mod Htrace;
pub mod login;
//pub mod ApiError;

// LOGIN and SIGNUP const
pub const SESSION_LOGIN_NBTRY: &str = "SESSION_LOGIN_NBTRY";
pub const SESSION_LOGIN_NBTRY_LAST: &str = "SESSION_LOGIN_NBTRY_LAST";
pub const SESSION_LOGIN_NBTRY_MAX: u8 = 3;
pub const SESSION_LOGIN_NBTRY_DELAY_RESET: Duration = Duration::from_secs(15*60); // 15 minutes
pub const SESSION_SIGN_NBTRY: &str = "SESSION_SIGN_NBTRY";
pub const SESSION_SIGN_NBTRY_LAST: &str = "SESSION_SIGN_NBTRY_LAST";
pub const SESSION_SIGN_NBTRY_MAX: u8 = 1;
pub const SESSION_SIGN_NBTRY_DELAY_RESET: Duration = Duration::from_secs(3600*24); // 1 day
