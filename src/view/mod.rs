//! Faces. A rig's `ui` field selects one of these modules; a view reads the
//! engine's outcome and owns everything the user sees (ADR 0002).

pub mod oneshot;
pub mod transcript;
pub mod ui_text;
