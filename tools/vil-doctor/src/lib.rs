//! VIL Doctor engine shared by the window (`vil-doctor`) and console (`vil-doctor-cli`) front ends.

pub mod checks;
pub mod console;
pub mod engine;
pub mod model;
pub mod paths;
pub mod ps;
pub mod report;
pub mod text;
pub mod win;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
