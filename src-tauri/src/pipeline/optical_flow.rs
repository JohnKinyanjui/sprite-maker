//! Optical-flow transition repair is permanently deferred (G29 Path B).
//!
//! The supported transition path uses rig midpoint interpolation (`quality::rig_bridge`),
//! motion-aware midpoint blending, and rig-rendered in-between frames (G27).

pub fn optical_flow_enabled() -> bool {
    false
}

pub fn optical_flow_status() -> &'static str {
    "optical flow permanently deferred — use rig bridge, motion-aware midpoint, or rig-rendered in-betweens"
}
