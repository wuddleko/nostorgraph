#[macro_use]
extern crate rocket;

mod access_log;
pub(crate) mod config;
mod crawl;
pub(crate) mod csrf;
pub(crate) use client as nostr;
pub mod open;
mod pages;
pub(crate) mod relay_set;
pub(crate) mod relays;
pub(crate) mod seal;
mod server;
pub(crate) mod template;
pub(crate) mod tor_circuits;

pub(crate) use server::parse_frontmatter;
pub(crate) use server::render_options;

pub use server::launch;
