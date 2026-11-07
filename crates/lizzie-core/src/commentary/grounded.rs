use super::{verify, ChatTransport, CommentaryFailure, CommentaryJob, Message};
use std::sync::atomic::Ordering;
pub const CORRECTION: &str = "Correct the unsupported claims below using ONLY the original frozen evidence. Do not add moves or substitute arbitrary coordinates. Remove unsupported claims, retain useful board reasoning, and return the complete corrected lesson in the requested language.\n";
pub fn run(transport: &mut dyn ChatTransport, job: &CommentaryJob) -> Result<String, CommentaryFailure> {
    let mut messages = job.messages.clone();
    for attempt in 0..2 {
        if job.cancel.load(Ordering::Acquire) {
            return Err(CommentaryFailure::cancelled());
        }
        let result = transport.complete(&messages, &job.cancel)?;
        if job.cancel.load(Ordering::Acquire) {
            return Err(CommentaryFailure::cancelled());
        }
        let check = verify::verify(&result, &job.positions);
        if check.ok() {
            return Ok(result);
        }
        if attempt == 0 {
            messages.push(Message::new("assistant", result));
            messages.push(Message::new(
                "user",
                format!(
                    "{CORRECTION}{}",
                    check.violations.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
                ),
            ));
        }
    }
    Err(CommentaryFailure::keyed(
        "Teacher.learning.unverified",
        "The explanation could not be verified against this position. Nothing was saved.",
    ))
}
