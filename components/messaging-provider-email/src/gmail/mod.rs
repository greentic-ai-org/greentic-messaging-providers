//! Gmail provider support.
//!
//! - `push`: Pub/Sub push body parsing + inbound verification gate.
//! - `fetch`: Google OAuth token acquisition + Gmail API fetch (history/messages).

pub mod fetch;
pub mod push;
