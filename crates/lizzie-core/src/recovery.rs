//! 存档与读档 storage (Java `LizzieFrame.saveTempGame` / `saveAutoGame`, `save/save`): the slots
//! of this product live in its own data directory, never in Java's.
//!
//! - Manual slot `n` (存档n, from 1) is `game{n}.sgf` with its main-board picture `game{n}.png`.
//!   Deleting one moves the later ones up, so the numbers stay 1..=count.
//! - The automatic slot written on exit is `autoGame1.*`; the periodic one (every 60 seconds)
//!   is `autoGame2.*`. A periodic save marks the session unclean until the exit save clears it;
//!   a start that finds it still unclean (a crash or a killed process) promotes the periodic save
//!   to the automatic slot, like Java's start-up rotation.
//! - `save.json` lists the slots: name, local time, move number and the position the record was
//!   at (child indexes from the root).
//!
//! Objective corrections against the fixed Java build: the rotation happens before the optional
//! resume, so a crash resumes the newer periodic save rather than the older exit save; it works
//! on every platform (Java used `save\\autoGame1.sgf`, a Windows-only path); and the rotated slot
//! carries the periodic save's own time and position (Java kept the old exit save's).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::{RecoveryView, SlotRef, SlotView};
use crate::formats;
use crate::record::{GameRecord, NodeId, ROOT};
use crate::settings::{self, RecoverySettings};

pub const INDEX_FILE: &str = "save.json";

/// Child indexes from the root to `node`: where a slot's record was.
pub fn child_path(rec: &GameRecord, node: NodeId) -> Vec<u32> {
    rec.path_to(node)
        .windows(2)
        .map(|w| rec.node(w[0]).children.iter().position(|&c| c == w[1]).unwrap_or(0) as u32)
        .collect()
}

/// The node at child indexes `path`, stopping where the record has no such child.
pub fn follow(rec: &GameRecord, path: &[u32]) -> NodeId {
    let mut id = ROOT;
    for &i in path {
        match rec.node(id).children.get(i as usize) {
            Some(&child) => id = child,
            None => break,
        }
    }
    id
}

/// What the panel shows of a slot and where its record was.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SlotMeta {
    pub name: String,
    /// `yyyy-MM-dd HH:mm:ss`, local.
    pub time: String,
    pub move_number: u32,
    /// Child indexes from the root to the position at save time.
    pub path: Vec<u32>,
    pub thumbnail: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Index {
    slots: Vec<SlotMeta>,
    /// `autoGame1`: written on exit (or promoted from a periodic save).
    exit: Option<SlotMeta>,
    /// `autoGame2`: written every 60 seconds.
    periodic: Option<SlotMeta>,
    /// A periodic save happened after the last exit save.
    unclean: bool,
}

/// Which automatic save is being written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoSave {
    Exit,
    Periodic,
}

/// One slot to be written: its record text and picture.
pub struct SlotWrite<'a> {
    pub meta: SlotMeta,
    pub sgf: &'a [u8],
    pub png: Option<&'a [u8]>,
}

pub struct Store {
    dir: PathBuf,
    index: Index,
    revision: u64,
}

fn io_error(context: &str, e: std::io::Error) -> String {
    format!("{context}: {e}")
}

impl Store {
    /// Loads the slot list (missing or unreadable: none) and performs the start-up rotation.
    pub fn open(dir: &Path) -> Store {
        let index: Index = settings::load_json(&dir.join(INDEX_FILE));
        let mut store = Store {
            dir: dir.to_path_buf(),
            index,
            revision: 1,
        };
        if store.index.unclean {
            if let Some(periodic) = store.index.periodic.take() {
                let moved = ["sgf", "png"].iter().all(|ext| {
                    let from = store.auto_file(AutoSave::Periodic, ext);
                    let to = store.auto_file(AutoSave::Exit, ext);
                    let _ = std::fs::remove_file(&to);
                    !from.exists() || std::fs::rename(&from, &to).is_ok()
                });
                if moved {
                    store.index.exit = Some(periodic);
                }
            }
            store.index.unclean = false;
            let _ = store.save_index();
        }
        store
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn save_index(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        settings::save_json(&self.dir.join(INDEX_FILE), &self.index)
    }

    fn game_file(&self, index: u32, ext: &str) -> PathBuf {
        self.dir.join(format!("game{index}.{ext}"))
    }

    fn auto_file(&self, which: AutoSave, ext: &str) -> PathBuf {
        let n = match which {
            AutoSave::Exit => 1,
            AutoSave::Periodic => 2,
        };
        self.dir.join(format!("autoGame{n}.{ext}"))
    }

    pub fn view(&self, settings: &RecoverySettings) -> RecoveryView {
        let view = |slot: SlotRef, m: &SlotMeta| SlotView {
            slot,
            name: if slot == SlotRef::Auto {
                String::new()
            } else {
                m.name.clone()
            },
            time: m.time.clone(),
            move_number: m.move_number,
            has_thumbnail: m.thumbnail,
        };
        RecoveryView {
            settings: settings.clone(),
            auto: self.index.exit.as_ref().map(|m| view(SlotRef::Auto, m)),
            slots: self
                .index
                .slots
                .iter()
                .enumerate()
                .map(|(i, m)| view(SlotRef::Manual { index: i as u32 + 1 }, m))
                .collect(),
            revision: self.revision,
        }
    }

    pub fn manual_count(&self) -> u32 {
        self.index.slots.len() as u32
    }

    /// The record file and saved state of a slot that exists.
    pub fn slot(&self, slot: SlotRef) -> Option<(PathBuf, &SlotMeta)> {
        match slot {
            SlotRef::Auto => self
                .index
                .exit
                .as_ref()
                .map(|m| (self.auto_file(AutoSave::Exit, "sgf"), m)),
            SlotRef::Manual { index } => index
                .checked_sub(1)
                .and_then(|i| self.index.slots.get(i as usize))
                .map(|m| (self.game_file(index, "sgf"), m)),
        }
    }

    /// The record to resume at start (`resumeFile`): the automatic slot, else the periodic save.
    pub fn resume_file(&self) -> Option<PathBuf> {
        let exit = self.auto_file(AutoSave::Exit, "sgf");
        let periodic = self.auto_file(AutoSave::Periodic, "sgf");
        if self.index.exit.is_some() && exit.is_file() {
            Some(exit)
        } else if self.index.periodic.is_some() && periodic.is_file() {
            Some(periodic)
        } else {
            None
        }
    }

    pub fn thumbnail(&self, slot: SlotRef) -> Option<Vec<u8>> {
        let file = match slot {
            SlotRef::Auto => self.auto_file(AutoSave::Exit, "png"),
            SlotRef::Manual { index } => self.game_file(index, "png"),
        };
        let (_, meta) = self.slot(slot)?;
        meta.thumbnail.then(|| std::fs::read(file).ok()).flatten()
    }

    fn write_files(&self, sgf_file: &Path, png_file: &Path, write: &SlotWrite) -> Result<bool, String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| io_error("creating the save folder failed", e))?;
        formats::write_file_atomic(sgf_file, write.sgf).map_err(|e| io_error("writing the slot failed", e))?;
        Ok(match write.png {
            Some(png) => formats::write_file_atomic(png_file, png).is_ok(),
            None => {
                let _ = std::fs::remove_file(png_file);
                false
            }
        })
    }

    /// 新建存档: appends a manual slot.
    pub fn add(&mut self, write: SlotWrite) -> Result<(), String> {
        let index = self.manual_count() + 1;
        let (sgf, png) = (self.game_file(index, "sgf"), self.game_file(index, "png"));
        let thumbnail = self.write_files(&sgf, &png, &write)?;
        self.index.slots.push(SlotMeta {
            thumbnail,
            ..write.meta
        });
        if let Err(e) = self.save_index() {
            self.index.slots.pop();
            let _ = std::fs::remove_file(&sgf);
            let _ = std::fs::remove_file(&png);
            return Err(io_error("writing the slot list failed", e));
        }
        self.revision += 1;
        Ok(())
    }

    /// 保存 on an existing manual slot.
    pub fn overwrite(&mut self, index: u32, write: SlotWrite) -> Result<(), String> {
        if self.slot(SlotRef::Manual { index }).is_none() {
            return Err(format!("no slot {index}"));
        }
        let (sgf, png) = (self.game_file(index, "sgf"), self.game_file(index, "png"));
        let thumbnail = self.write_files(&sgf, &png, &write)?;
        let i = index as usize - 1;
        let before = std::mem::replace(
            &mut self.index.slots[i],
            SlotMeta {
                thumbnail,
                ..write.meta
            },
        );
        self.revision += 1;
        if let Err(e) = self.save_index() {
            self.index.slots[i] = before;
            return Err(io_error("writing the slot list failed", e));
        }
        Ok(())
    }

    pub fn rename(&mut self, index: u32, name: &str) -> Result<bool, String> {
        let Some(i) = index
            .checked_sub(1)
            .map(|i| i as usize)
            .filter(|&i| i < self.index.slots.len())
        else {
            return Err(format!("no slot {index}"));
        };
        if self.index.slots[i].name == name {
            return Ok(false);
        }
        let before = std::mem::replace(&mut self.index.slots[i].name, name.to_string());
        if let Err(e) = self.save_index() {
            self.index.slots[i].name = before;
            return Err(io_error("writing the slot list failed", e));
        }
        self.revision += 1;
        Ok(true)
    }

    /// 删除: a manual slot's files go and the later slots move up (`deleteTempGame`); the
    /// automatic slot disappears from the panel and from resuming.
    pub fn delete(&mut self, slot: SlotRef) -> Result<bool, String> {
        match slot {
            SlotRef::Auto => {
                if self.index.exit.take().is_none() {
                    return Ok(false);
                }
                for ext in ["sgf", "png"] {
                    let _ = std::fs::remove_file(self.auto_file(AutoSave::Exit, ext));
                }
            }
            SlotRef::Manual { index } => {
                let count = self.manual_count();
                if index == 0 || index > count {
                    return Ok(false);
                }
                for ext in ["sgf", "png"] {
                    let _ = std::fs::remove_file(self.game_file(index, ext));
                    for later in index + 1..=count {
                        let from = self.game_file(later, ext);
                        if from.exists() {
                            std::fs::rename(&from, self.game_file(later - 1, ext))
                                .map_err(|e| io_error("renumbering the slots failed", e))?;
                        }
                    }
                }
                self.index.slots.remove(index as usize - 1);
            }
        }
        self.revision += 1;
        self.save_index()
            .map_err(|e| io_error("writing the slot list failed", e))?;
        Ok(true)
    }

    /// 全部删除: every manual slot (`deleteAllTempGame`).
    pub fn delete_all(&mut self) -> Result<bool, String> {
        if self.index.slots.is_empty() {
            return Ok(false);
        }
        for index in 1..=self.manual_count() {
            for ext in ["sgf", "png"] {
                let _ = std::fs::remove_file(self.game_file(index, ext));
            }
        }
        self.index.slots.clear();
        self.revision += 1;
        self.save_index()
            .map_err(|e| io_error("writing the slot list failed", e))?;
        Ok(true)
    }

    /// `saveAutoGame(1)` on exit / `saveAutoGame(2)` every 60 seconds.
    pub fn write_auto(&mut self, which: AutoSave, write: SlotWrite) -> Result<(), String> {
        let (sgf, png) = (self.auto_file(which, "sgf"), self.auto_file(which, "png"));
        let thumbnail = self.write_files(&sgf, &png, &write)?;
        let meta = Some(SlotMeta {
            thumbnail,
            ..write.meta
        });
        match which {
            AutoSave::Exit => {
                self.index.exit = meta;
                self.index.unclean = false;
            }
            AutoSave::Periodic => {
                self.index.periodic = meta;
                self.index.unclean = true;
            }
        }
        self.revision += 1;
        self.save_index()
            .map_err(|e| io_error("writing the slot list failed", e))
    }

    /// Adds imported slots after the existing ones, all or nothing: on failure the files written
    /// so far are removed and the list is unchanged. Returns how many were added.
    pub fn append_all(&mut self, writes: &[SlotWrite]) -> Result<usize, String> {
        if writes.is_empty() {
            return Ok(0);
        }
        let first = self.manual_count() + 1;
        let mut written: Vec<PathBuf> = Vec::new();
        let mut metas = Vec::new();
        let undo = |written: &[PathBuf]| {
            for f in written {
                let _ = std::fs::remove_file(f);
            }
        };
        for (i, write) in writes.iter().enumerate() {
            let index = first + i as u32;
            let (sgf, png) = (self.game_file(index, "sgf"), self.game_file(index, "png"));
            match self.write_files(&sgf, &png, write) {
                Ok(thumbnail) => {
                    written.push(sgf);
                    if thumbnail {
                        written.push(png);
                    }
                    metas.push(SlotMeta {
                        thumbnail,
                        ..write.meta.clone()
                    });
                }
                Err(e) => {
                    undo(&written);
                    return Err(e);
                }
            }
        }
        let before = self.index.slots.len();
        self.index.slots.extend(metas);
        if let Err(e) = self.save_index() {
            self.index.slots.truncate(before);
            undo(&written);
            return Err(io_error("writing the slot list failed", e));
        }
        self.revision += 1;
        Ok(writes.len())
    }

    /// Takes back [`Self::append_all`] (a later step of the same import failed).
    pub fn remove_last(&mut self, count: usize) {
        let total = self.manual_count();
        for index in (total + 1 - count as u32)..=total {
            for ext in ["sgf", "png"] {
                let _ = std::fs::remove_file(self.game_file(index, ext));
            }
        }
        let keep = self.index.slots.len() - count;
        self.index.slots.truncate(keep);
        self.revision += 1;
        let _ = self.save_index();
    }
}

/// Local wall-clock time as Java's `yyyy-MM-dd HH:mm:ss`.
pub fn now_text() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
