#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub(crate) mod actions;
mod adapter;
pub(crate) mod input;
pub(crate) mod notifications;
pub(crate) mod system;
pub(crate) mod tree;

pub use adapter::MobileAdapter;
pub use system::config::MobileConfig;
pub use system::platform::MobilePlatform;
