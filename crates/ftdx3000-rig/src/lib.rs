//! FTDX3000 rig engine and port sharing.

pub mod detect;
pub mod engine;
pub mod sim;
pub mod transport;

pub use engine::{CatError, EngineConfig, Freshness, LinkState, Rig, Source};
pub mod pty;
pub mod vport;
pub mod rigctld;
pub mod services;

pub use services::{Config, Macro, Services, UiPrefs};
pub mod audio;
pub mod remote;
pub mod spectrum;
