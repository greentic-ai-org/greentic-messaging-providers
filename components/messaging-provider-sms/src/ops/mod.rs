//! Provider operations for the SMS (Twilio) messaging component.
//!
//! Egress (`render_plan`/`encode`/`send_payload`) is filled in by later tasks
//! in this epic; inbound parse + `X-Twilio-Signature` validation are done.

mod encode;
mod ingest;
mod render;
mod send_payload;
mod signature;
mod webhook;

pub(crate) use encode::encode_op;
pub(crate) use ingest::ingest_http;
pub(crate) use render::render_plan;
pub(crate) use send_payload::send_payload;
pub(crate) use webhook::setup_webhook;
