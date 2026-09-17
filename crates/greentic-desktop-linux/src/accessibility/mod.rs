//! AT-SPI element automation shared by the Linux X11 and Wayland adapters.

mod choice;
pub mod executor;
#[cfg(target_os = "linux")]
pub mod live;
pub mod locator;
pub mod model;
pub mod operations;
#[cfg(not(target_os = "linux"))]
mod unavailable;

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;

pub use executor::{AccessibilityExecutor, AccessibilityTiming};
#[cfg(target_os = "linux")]
pub use live::{AtspiBackend, AtspiHandle};
pub use model::{
    AccessibleBackend, AccessibleNodeInfo, NodeInterfaces, NodeStates, SnapshotNode, TreeSnapshot,
    WalkLimits,
};
#[cfg(not(target_os = "linux"))]
pub use unavailable::UnavailableBackend;
