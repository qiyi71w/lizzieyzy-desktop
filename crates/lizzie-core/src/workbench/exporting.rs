//! 文件 > 更多保存 and the clipboard copies as application operations.
//!
//! A record export is captured like a save (`SgfSaveCoordinator.chooseAndSave` captures on the
//! event thread) and written by the same serial writer; its receipt ([`Workbench::complete_save`])
//! confirms no revision. Pictures are written by the adapter; the workbench remembers the folder.

use std::path::{Path, PathBuf};

use super::{PendingSave, SaveJob, Workbench};
use crate::api::{CommentLabels, ExportKind, NodeAnalysisView};
use crate::formats::saved_analysis;
use crate::formats::sgf_write::{self, Export};
use crate::formats::winrate_comment::{self, NodeEval};
use crate::record::{GameRecord, NodeId};
use crate::viewpoint::Viewpoint;

fn eval(a: &NodeAnalysisView) -> NodeEval {
    NodeEval {
        engine: a.engine.clone(),
        visits: a.visits,
        winrate: a.winrate,
        score_lead: a.score_lead,
        score_stdev: a.score_stdev,
        komi: a.komi,
        // Live results carry no PDA of their own.
        pda: 0.0,
        to_play: a.to_play,
    }
}

/// The analysis saved with node `id` in the record (KataGo data always has an uncertainty, as in
/// Java's `isKataData` summary).
fn saved(rec: &GameRecord, id: NodeId) -> Option<NodeEval> {
    let node = rec.node(id);
    let s = saved_analysis::primary(&node.properties)?;
    Some(NodeEval {
        engine: s.engine,
        visits: s.visits,
        winrate: s.winrate,
        score_stdev: s.score_lead.map(|_| s.score_stdev),
        score_lead: s.score_lead,
        komi: rec.info.komi,
        pda: s.pda,
        to_play: node.to_play,
    })
}

impl Workbench {
    /// The export text of the current record.
    pub fn export_text(&self, kind: ExportKind, labels: &CommentLabels) -> String {
        let rec = &self.doc.record;
        match kind {
            ExportKind::Raw => sgf_write::write_export(rec, &Export::Raw),
            ExportKind::Branch => sgf_write::write_export(rec, &Export::Branch { current: self.cursor }),
            ExportKind::RawWithComments => {
                // An analysed node (playouts > 0) gets its summary (`appendRawSerializedComment`):
                // this session's analysis or the analysis read with the record, whichever has more
                // visits (Java keeps one `BoardData` result per node and replaces it only with
                // more visits).
                let analysis = |id: NodeId| {
                    let live = self.store.get(rec, id).map(eval);
                    let saved = saved(rec, id);
                    match (live, saved) {
                        (Some(l), Some(s)) => Some(if s.visits > l.visits { s } else { l }),
                        (l, s) => l.or(s),
                    }
                    .filter(|a| a.visits > 0)
                };
                let view = Viewpoint::from(&self.config.display);
                let comment = |id: NodeId| {
                    let a = analysis(id)?;
                    let previous = rec.node(id).parent.map(analysis);
                    Some(winrate_comment::format(
                        labels,
                        view,
                        &a,
                        previous.as_ref().map(Option::as_ref),
                        &rec.node(id).comment,
                    ))
                };
                sgf_write::write_export(rec, &Export::RawWithComments { comment: &comment })
            }
        }
    }

    /// Captures export `kind` of the current record for the serial writer, like
    /// [`Workbench::begin_save`].
    pub fn begin_export(&mut self, kind: ExportKind, labels: &CommentLabels, path: &str) -> SaveJob {
        let ticket = self.next_save;
        self.next_save += 1;
        self.saves.push(PendingSave {
            ticket,
            document: self.doc.id,
            revision: self.doc.revision,
            path: path.to_string(),
            export: Some(kind),
        });
        SaveJob {
            ticket,
            path: PathBuf::from(path),
            text: self.export_text(kind, labels),
        }
    }

    /// 复制棋谱到剪贴板 (`SGFParser.saveToString(false)`): the record as a save writes it.
    pub fn record_text(&self, application: &str) -> String {
        sgf_write::write(&self.doc.record, application)
    }

    /// Where the picture chooser starts: the last picture folder, else the last record folder
    /// (`filesystem.optString("last-image-folder", last-folder)`).
    pub fn picture_folder(&self) -> Option<String> {
        self.persisted
            .last_image_folder
            .clone()
            .or_else(|| self.persisted.last_folder.clone())
    }

    /// The picture chooser was confirmed with `path` (`filesystem.put("last-image-folder", …)`,
    /// before the write and any replace prompt).
    pub fn remember_picture_folder(&mut self, path: &str) {
        if let Some(parent) = Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()) {
            let folder = parent.to_string_lossy().into_owned();
            if self.persisted.last_image_folder.as_deref() != Some(&folder) {
                self.persisted.last_image_folder = Some(folder);
                let _ = self.save_persisted();
            }
        }
    }
}
