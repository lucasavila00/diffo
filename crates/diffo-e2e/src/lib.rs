#![doc = include_str!("../README.md")]

mod git_proxy;
mod input;
mod reader;
mod screen;
mod selectors;
mod types;

pub use git_proxy::{GitGatePhase, GitProxy};
pub use screen::DiffoScreen;
pub use types::{Key, ScrollDirection, Selector};
