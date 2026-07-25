#![doc = include_str!("../README.md")]

pub const BUILD_SHA: &str = env!("DIFFO_BUILD_SHA");

pub mod diff;
pub mod explorer;
pub mod workbench;

pub use diff::*;
pub use workbench::*;
