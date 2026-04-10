//! Provider operations for the WhatsApp messaging component.
//!
//! This module is split across multiple files for maintainability. The public
//! surface remains unchanged — `lib.rs` calls into these functions exactly as
//! it did when they lived in a single `ops.rs` file.

mod encode;
mod ingest;
mod render;
mod reply;
mod send;
mod send_payload;

pub(crate) use encode::encode_op;
pub(crate) use ingest::ingest_http;
pub(crate) use render::render_plan;
pub(crate) use reply::handle_reply;
pub(crate) use send::handle_send;
pub(crate) use send_payload::send_payload;
