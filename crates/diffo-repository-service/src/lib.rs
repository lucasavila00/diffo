#![doc = include_str!("../README.md")]

mod service;
mod watcher;
mod worker;

pub use service::{RepositoryEvent, RepositoryService};
