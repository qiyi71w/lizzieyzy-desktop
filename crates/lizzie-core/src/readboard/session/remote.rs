//! What the helper said about the remote window during the current frame (Java
//! `SyncRemoteContext`): the platform, the live room or record view and its move counters. A frame
//! uses it to recognise an existing node or to choose the rebuilt position's move number; `end`
//! resets it.

use crate::readboard::protocol::{java_trim, LastMoveSource, SyncPlatform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowKind {
    LiveRoom,
    RecordView,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RemoteContext {
    pub platform: SyncPlatform,
    pub window: WindowKind,
    pub fox_move_number: Option<i32>,
    pub room_token: Option<String>,
    pub live_title_move: Option<i32>,
    pub record_current_move: Option<i32>,
    pub record_total_move: Option<i32>,
    pub record_at_end: bool,
    pub title_fingerprint: Option<String>,
    pub last_move_source: LastMoveSource,
    pub force_rebuild: bool,
}

/// `normalizeText`: trimmed, empty means absent.
fn normalize(text: Option<String>) -> Option<String> {
    text.and_then(|t| {
        let t = java_trim(&t);
        (!t.is_empty()).then(|| t.to_owned())
    })
}

impl RemoteContext {
    pub fn generic(force_rebuild: bool) -> RemoteContext {
        RemoteContext {
            platform: SyncPlatform::Generic,
            window: WindowKind::Unknown,
            fox_move_number: None,
            room_token: None,
            live_title_move: None,
            record_current_move: None,
            record_total_move: None,
            record_at_end: false,
            title_fingerprint: None,
            last_move_source: LastMoveSource::LegacyUnknown,
            force_rebuild,
        }
    }

    pub fn with_platform(&self, platform: SyncPlatform) -> RemoteContext {
        if platform == SyncPlatform::Generic {
            return RemoteContext::generic(self.force_rebuild);
        }
        RemoteContext {
            platform,
            window: WindowKind::Unknown,
            fox_move_number: self.fox_move_number,
            last_move_source: self.last_move_source,
            force_rebuild: self.force_rebuild,
            ..RemoteContext::generic(false)
        }
    }

    pub fn with_fox_move_number(&self, n: Option<i32>) -> RemoteContext {
        RemoteContext { fox_move_number: n, ..self.clone() }
    }

    pub fn with_last_move_source(&self, source: LastMoveSource) -> RemoteContext {
        RemoteContext { last_move_source: source, ..self.clone() }
    }

    /// Room lines switch to the live-room window and forget record-view fields.
    fn live_room(&self, room_token: Option<String>, live_title_move: Option<i32>) -> RemoteContext {
        RemoteContext {
            window: WindowKind::LiveRoom,
            room_token: normalize(room_token),
            live_title_move,
            record_current_move: None,
            record_total_move: None,
            record_at_end: false,
            title_fingerprint: None,
            ..self.clone()
        }
    }

    /// Record lines switch to the record-view window and forget live-room fields.
    fn record_view(
        &self,
        current: Option<i32>,
        total: Option<i32>,
        at_end: bool,
        fingerprint: Option<String>,
    ) -> RemoteContext {
        RemoteContext {
            window: WindowKind::RecordView,
            room_token: None,
            live_title_move: None,
            record_current_move: current,
            record_total_move: total,
            record_at_end: at_end,
            title_fingerprint: normalize(fingerprint),
            ..self.clone()
        }
    }

    pub fn with_room_token(&self, token: String) -> RemoteContext {
        self.live_room(Some(token), self.live_title_move)
    }

    pub fn with_live_title_move(&self, n: Option<i32>) -> RemoteContext {
        self.live_room(self.room_token.clone(), n)
    }

    pub fn with_record_current_move(&self, n: Option<i32>) -> RemoteContext {
        self.record_view(n, self.record_total_move, self.record_at_end, self.title_fingerprint.clone())
    }

    pub fn with_record_total_move(&self, n: Option<i32>) -> RemoteContext {
        self.record_view(self.record_current_move, n, self.record_at_end, self.title_fingerprint.clone())
    }

    pub fn with_record_at_end(&self, at_end: bool) -> RemoteContext {
        self.record_view(self.record_current_move, self.record_total_move, at_end, self.title_fingerprint.clone())
    }

    pub fn with_title_fingerprint(&self, fingerprint: String) -> RemoteContext {
        self.record_view(self.record_current_move, self.record_total_move, self.record_at_end, Some(fingerprint))
    }

    pub fn with_force_rebuild(&self, force_rebuild: bool) -> RemoteContext {
        RemoteContext { force_rebuild, ..self.clone() }
    }

    /// `supportsFoxRecovery`: a Fox window with a move number its title agrees with.
    pub fn supports_fox_recovery(&self) -> bool {
        self.recovery_move_number().is_some()
    }

    /// `recoveryMoveNumber`: the Fox move number, unless the window title names another one.
    pub fn recovery_move_number(&self) -> Option<i32> {
        if self.platform != SyncPlatform::Fox {
            return None;
        }
        let fox = self.fox_move_number?;
        match self.title_move_number() {
            Some(title) if title != fox => None,
            _ => Some(fox),
        }
    }

    fn title_move_number(&self) -> Option<i32> {
        match self.window {
            WindowKind::LiveRoom => self.live_title_move,
            WindowKind::RecordView => {
                if self.record_current_move.is_none() && self.record_at_end && self.record_total_move.is_some() {
                    self.record_total_move
                } else {
                    self.record_current_move
                }
            }
            WindowKind::Unknown => None,
        }
    }

    /// `conflictsWith`: another platform, window, room, or record.
    pub fn conflicts_with(&self, other: &RemoteContext) -> bool {
        if self.platform != other.platform || self.window != other.window {
            return true;
        }
        match self.window {
            WindowKind::LiveRoom => self.room_token != other.room_token,
            WindowKind::RecordView => {
                self.title_fingerprint != other.title_fingerprint || self.record_total_move != other.record_total_move
            }
            WindowKind::Unknown => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_at_end_falls_back_to_the_total_move_for_fox_recovery() {
        // ReadBoardFoxMoveNumberParsingTest.recordAtEndFallsBackToTotalMoveForFoxRecovery
        let ctx = RemoteContext::generic(false)
            .with_platform(SyncPlatform::Fox)
            .with_fox_move_number(Some(256))
            .with_record_total_move(Some(256))
            .with_record_at_end(true)
            .with_title_fingerprint("title".into());
        assert_eq!(ctx.recovery_move_number(), Some(256));
        let behind = ctx.with_record_total_move(Some(300));
        assert_eq!(behind.recovery_move_number(), None);
    }

    #[test]
    fn a_disagreeing_live_title_withdraws_the_fox_move_number() {
        let ctx = RemoteContext::generic(false)
            .with_platform(SyncPlatform::Fox)
            .with_room_token(" 43581号 ".into())
            .with_live_title_move(Some(3))
            .with_fox_move_number(Some(2));
        assert_eq!(ctx.room_token.as_deref(), Some("43581号"));
        assert!(!ctx.supports_fox_recovery());
        assert!(ctx.with_live_title_move(Some(2)).supports_fox_recovery());
        assert!(ctx.conflicts_with(&ctx.with_room_token("other".into())));
        assert!(!ctx.conflicts_with(&ctx.with_fox_move_number(Some(9))));
    }
}
