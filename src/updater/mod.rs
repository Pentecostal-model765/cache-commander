pub mod cache;
pub mod http;
pub mod version;

use std::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // fields consumed once `check` is implemented (Task 5)
pub struct UpdateInfo {
    pub latest: String,
    pub url: String,
}

#[derive(Debug)]
#[allow(dead_code)] // variant constructed in Task 5, matched on in Task 8
pub enum UpdateMsg {
    Available(UpdateInfo),
}

/// Placeholder — real signature lands in Task 7.
#[allow(dead_code)] // called from main.rs in Task 9
pub fn start(_config: &crate::config::Config) -> mpsc::Receiver<UpdateMsg> {
    let (_tx, rx) = mpsc::channel();
    rx
}
