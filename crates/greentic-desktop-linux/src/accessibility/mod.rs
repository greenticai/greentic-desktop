//! AT-SPI element automation shared by the Linux X11 and Wayland adapters.

pub mod executor;
pub mod locator;
pub mod model;
pub mod operations;

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;

pub use executor::{AccessibilityExecutor, AccessibilityTiming};
pub use model::{
    AccessibleBackend, AccessibleNodeInfo, NodeInterfaces, NodeStates, SnapshotNode, TreeSnapshot,
    WalkLimits,
};
