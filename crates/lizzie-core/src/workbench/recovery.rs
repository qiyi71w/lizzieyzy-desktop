//! 存档与读档 as application operations (Java `LizzieFrame.showTempGamePanel` and
//! `saveAutoGame`): slots hold the current record as a save writes it, the position it is at and
//! the main-board picture the UI took; loading a slot opens its record like a file without making
//! it the record's own file.

use std::path::{Path, PathBuf};

use super::{OpenOrigin, SlotPosition, Workbench};
use crate::api::{ApiError, ErrorKind, RecoveryCommand, RecoveryView, SlotRef};
use crate::formats::picture;
use crate::recovery::{self, AutoSave, SlotMeta, SlotWrite};

/// What a recovery command needs from the adapter next.
#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryStep {
    /// Done; whether anything changed.
    Changed(bool),
    /// Read slot record `path` off the UI thread and hand it back with `ticket`
    /// ([`Workbench::complete_open`]).
    Load { ticket: u64, path: PathBuf },
}

fn io(message: String) -> ApiError {
    ApiError::new(ErrorKind::Io, message)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Workbench {
    pub(super) fn recovery_view(&self) -> RecoveryView {
        self.recovery.view(&self.config.recovery)
    }

    /// The current record as a slot: its save text, the position and the time.
    fn slot_meta(&self, name: String) -> SlotMeta {
        let rec = &self.doc.record;
        SlotMeta {
            name,
            time: recovery::now_text(),
            move_number: rec.node(self.cursor).move_number,
            path: recovery::child_path(rec, self.cursor),
            thumbnail: false,
        }
    }

    /// Runs a 存档与读档 command. `application` is the `AP` written into slot records.
    pub fn recovery_command(&mut self, op: RecoveryCommand, application: &str) -> Result<RecoveryStep, ApiError> {
        let changed = match op {
            RecoveryCommand::NewSlot { name, thumbnail } => {
                let png = picture::png_from_base64(&thumbnail).ok();
                let sgf = self.record_text(application);
                let meta = self.slot_meta(name);
                self.recovery
                    .add(SlotWrite {
                        meta,
                        sgf: sgf.as_bytes(),
                        png: png.as_deref(),
                    })
                    .map_err(io)?;
                true
            }
            RecoveryCommand::OverwriteSlot { index, name, thumbnail } => {
                let png = picture::png_from_base64(&thumbnail).ok();
                let sgf = self.record_text(application);
                let meta = self.slot_meta(name);
                self.recovery
                    .overwrite(
                        index,
                        SlotWrite {
                            meta,
                            sgf: sgf.as_bytes(),
                            png: png.as_deref(),
                        },
                    )
                    .map_err(io)?;
                true
            }
            RecoveryCommand::RenameSlot { index, name } => self.recovery.rename(index, &name).map_err(io)?,
            RecoveryCommand::LoadSlot { slot } => {
                let Some((path, meta)) = self.recovery.slot(slot) else {
                    return Err(ApiError::new(ErrorKind::InvalidArgument, "no such slot"));
                };
                let origin = OpenOrigin::Recovery {
                    path: path.to_string_lossy().into_owned(),
                    name: file_name(&path),
                    position: SlotPosition::Path(meta.path.clone()),
                };
                let ticket = self.begin_open(origin);
                return Ok(RecoveryStep::Load { ticket, path });
            }
            RecoveryCommand::DeleteSlot { slot } => self.recovery.delete(slot).map_err(io)?,
            RecoveryCommand::DeleteAllSlots => self.recovery.delete_all().map_err(io)?,
            RecoveryCommand::SetSettings { settings } => {
                if self.config.recovery == settings {
                    false
                } else {
                    self.commit_config(|c| c.recovery = settings)?;
                    true
                }
            }
            RecoveryCommand::AutoSave { thumbnail } => {
                self.auto_save(AutoSave::Periodic, Some(&thumbnail), application)?;
                true
            }
            RecoveryCommand::ExitAutoSave { thumbnail } => {
                self.auto_save(AutoSave::Exit, thumbnail.as_deref(), application)?;
                true
            }
        };
        Ok(RecoveryStep::Changed(changed))
    }

    /// `saveAutoGame`: only while 自动存档 is on (`autoSaveOnExit`).
    pub fn auto_save(&mut self, which: AutoSave, thumbnail: Option<&str>, application: &str) -> Result<(), ApiError> {
        if !self.config.recovery.auto_save_on_exit {
            return Ok(());
        }
        let png = thumbnail.and_then(|t| picture::png_from_base64(t).ok());
        let sgf = self.record_text(application);
        let meta = self.slot_meta(String::new());
        self.recovery
            .write_auto(
                which,
                SlotWrite {
                    meta,
                    sgf: sgf.as_bytes(),
                    png: png.as_deref(),
                },
            )
            .map_err(io)
    }

    /// Whether the exit should write the automatic slot (自动存档 is on).
    pub fn auto_save_on_exit(&self) -> bool {
        self.config.recovery.auto_save_on_exit
    }

    /// 启动时自动恢复 (`Lizzie.java` start: `resumeFile` when no record was given): starts
    /// reading the automatic slot's record, to be shown at the end of its main line.
    pub fn begin_resume(&mut self) -> Option<(u64, PathBuf)> {
        if !self.config.recovery.resume_previous_game {
            return None;
        }
        let path = self.recovery.resume_file()?;
        let origin = OpenOrigin::Recovery {
            path: path.to_string_lossy().into_owned(),
            name: file_name(&path),
            position: SlotPosition::LineEnd,
        };
        Some((self.begin_open(origin), path))
    }

    /// The picture of a slot, if it has one.
    pub fn slot_thumbnail(&self, slot: SlotRef) -> Option<Vec<u8>> {
        self.recovery.thumbnail(slot)
    }
}
