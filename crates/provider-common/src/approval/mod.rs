//! Approval rail contract v2.
//!
//! Channel-agnostic half of the rail: parse a `greentic.approval.request.v1`
//! body, and build the `greentic.approval.response.v1` body that answers it.
//! Rendering the affordance and reading the channel's interactive payload back
//! belong to each provider component.
//!
//! Contract: `greentic-designer/docs/approval-rail-contract-v2.md`.
//!
//! Two rules the parsers below exist to enforce, because both have a wrong
//! reading that works most of the time:
//!
//! - Unknown keys are ignored in both directions. Every field is read
//!   independently, so a shape this build has never seen (the reserved
//!   per-approver token, a richer role vocabulary) degrades that one field
//!   rather than failing the whole delivery.
//! - `tier.position` — not `tier.level` — is what the token binds to, and its
//!   `null` is a bound value meaning "this gate has not escalated", never `0`.

mod request;
mod response;
mod token;

pub use request::{ApprovalRequest, Approvers, Routing, Tier};
pub use response::{ApprovalResponse, Decision};
pub use token::DecisionToken;

/// Subject the designer publishes approval requests on.
pub const REQUEST_SUBJECT: &str = "greentic.approval.request.v1";

/// Subject a collected human answer is published on.
pub const RESPONSE_SUBJECT: &str = "greentic.approval.response.v1";

/// The request header a response must echo. The designer routes on it alone.
pub const CORRELATION_ID_HEADER: &str = "Greentic-Correlation-Id";
