//! Provider operations for the SMS (Twilio) messaging component.
//!
//! Task 1 scaffolds these as stubs; inbound parse, signature validation, and
//! egress are filled in by later tasks in this epic.

mod encode;
mod ingest;
mod render;
mod send_payload;
mod webhook;

pub(crate) use encode::encode_op;
pub(crate) use ingest::ingest_http;
pub(crate) use render::render_plan;
pub(crate) use send_payload::send_payload;
pub(crate) use webhook::setup_webhook;
