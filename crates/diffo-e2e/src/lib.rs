#![doc = include_str!("../README.md")]

mod input;
mod reader;
mod screen;
mod selectors;
mod types;

pub use screen::DiffoScreen;
pub use types::{Key, ScrollDirection, Selector};
