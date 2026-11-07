//! Fox AI 1.05 framing, authoritative game facts, and loopback transport.
//!
//! A physical submission is not a confirmed action. Only `Snapshot::actions`
//! contains observed actions; placement numbering deliberately excludes PASS.
mod protocol;
mod state;
mod transport;

pub use protocol::{encode, Framer};
pub use state::{Action, Clock, GameState, Placement, Rules, Snapshot, Submission, SubmissionFact, SubmissionOutcome};
pub use transport::{Listener, TransportEvent};
