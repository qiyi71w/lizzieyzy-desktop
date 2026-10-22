//! 文件 > 更多保存 and the clipboard copies as application operations.
//!
//! A record export is captured like a save (`SgfSaveCoordinator.chooseAndSave` captures on the
//! event thread) and written by the same serial writer; its receipt ([`Workbench::complete_save`])
//! confirms no revision. Pictures are written by the adapter; the workbench remembers the folder.

use std::path::{Path, PathBuf};

use super::analysis::AnalysisStore;
use super::{PendingSave, SaveJob, Workbench};
use crate::api::{CommentLabels, ExportKind, NodeAnalysisView};
use crate::formats::sgf_write::{self, Analysis, Export};
use crate::formats::winrate_comment::{self, NodeEval};
use crate::record::{DocumentEdit, NodeId, NodeKind, ROOT};
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

/// Use the same fixed-Java localization catalog as the UI, not a second string table.
fn comment_labels(language: crate::settings::Language) -> CommentLabels {
    static CATALOG: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../../ui/src/i18n/javaStrings.json"))
            .expect("bundled Java localization catalog")
    });
    use crate::settings::Language;
    let lang = match language {
        Language::System | Language::ZhCn => "ZhCn",
        Language::EnUs => "EnUs",
        Language::KoKr => "KoKr",
        Language::JaJp => "JaJp",
        Language::ZhTw => "ZhTw",
        Language::ThTh => "ThTh",
    };
    let strings = &CATALOG["strings"][lang];
    let label = |key: &str| {
        strings[key]
            .as_str()
            .expect("SGF comment label in every bundled locale")
            .to_string()
    };
    CommentLabels {
        black: label("SGFParse.black"),
        white: label("SGFParse.white"),
        winrate: label("SGFParse.winrate"),
        lead: label("SGFParse.leadJustScore"),
        lead_with_komi: label("SGFParse.leadWithKomi"),
        stdev: label("SGFParse.stdev"),
        playouts: label("SGFParse.playouts"),
        komi: label("SGFParse.komi"),
        pda: label("SGFParse.pda"),
    }
}

impl Workbench {
    fn node_eval(&self, store: &AnalysisStore, id: NodeId) -> Option<NodeEval> {
        let mut a = eval(store.get(&self.doc.record, id)?);
        a.pda = store.pda(id);
        (a.visits > 0).then_some(a)
    }

    /// The node's comment with its analysis summary (`formatComment`); in 双引擎模式 a node the
    /// secondary engine analysed gets that engine's summary instead (`formatComment2`), its
    /// changes taken against the secondary engine's result of the previous node.
    fn formatted_comment(&self, id: NodeId, labels: &CommentLabels) -> Option<String> {
        let rec = &self.doc.record;
        let secondary = self.double_engine() && self.node_eval(&self.secondary_store, id).is_some();
        let store = if secondary { &self.secondary_store } else { &self.store };
        let a = self.node_eval(store, id)?;
        let previous = rec.node(id).parent.map(|p| self.node_eval(store, p));
        Some(winrate_comment::format(
            labels,
            Viewpoint::from(&self.config.display),
            &a,
            previous.as_ref().map(Option::as_ref),
            &rec.node(id).comment,
        ))
    }

    /// Both engines' saved-analysis payloads (`LZ`/`LZOP`, `LZ2`/`LZOP2`).
    fn write_payloads<T>(&self, write: impl FnOnce(Analysis) -> T) -> T {
        let rec = &self.doc.record;
        let primary = |id| self.store.payload(rec, id);
        let secondary = |id| self.secondary_store.payload(rec, id);
        write(Analysis {
            primary: &primary,
            secondary: &secondary,
        })
    }

    /// The comment pane while an engine is loaded (`LizzieFrame.appendComment` writes the
    /// summary into the current move's comment; `setComment` shows the previous move's while
    /// the current one has neither comment nor summary yet).
    pub(super) fn comment_with_analysis(&self) -> Option<String> {
        if !self.config.record.append_winrate_to_comment || self.engines.foreground().is_none() {
            return None;
        }
        let rec = &self.doc.record;
        let labels = comment_labels(self.active_language);
        let summary = |id: NodeId| {
            (id != ROOT && rec.node(id).kind != NodeKind::Snapshot)
                .then(|| self.formatted_comment(id, &labels))
                .flatten()
        };
        summary(self.cursor).or_else(|| {
            let parent = rec.parent(self.cursor)?;
            (rec.node(self.cursor).comment.is_empty() && self.engines.analysing(self.doc.id, self.cursor))
                .then(|| summary(parent).unwrap_or_else(|| rec.node(parent).comment.clone()))
        })
    }

    /// Java mutates move comments on save, but never snapshot comments. Do so through the
    /// document's write entry before capturing the revision that the save will confirm.
    pub(super) fn prepare_save_comments(&mut self) {
        if !self.config.record.append_winrate_to_comment {
            return;
        }
        let labels = comment_labels(self.active_language);
        let mut pending = vec![ROOT];
        let mut edits = Vec::new();
        while let Some(id) = pending.pop() {
            let node = self.doc.record.node(id);
            pending.extend(node.children.iter().copied());
            if id != ROOT && node.kind != NodeKind::Snapshot {
                if let Some(comment) = self.formatted_comment(id, &labels).filter(|c| *c != node.comment) {
                    edits.push(DocumentEdit::SetComment { node: id, comment });
                }
            }
        }
        for edit in edits {
            self.edit_document(edit).expect("save comments target existing nodes");
        }
    }
    /// The export text of the current record.
    pub fn export_text(&self, kind: ExportKind, labels: &CommentLabels) -> String {
        let rec = &self.doc.record;
        match kind {
            ExportKind::Raw => self.write_payloads(|a| sgf_write::write_export_analyzed(rec, &Export::Raw, Some(a))),
            ExportKind::Branch => sgf_write::write_export(rec, &Export::Branch { current: self.cursor }),
            ExportKind::RawWithComments => {
                // The store is the one BoardData result: imported data was loaded into it and
                // live analysis replaces it only when stronger.
                let comment = |id| self.formatted_comment(id, labels);
                self.write_payloads(|a| {
                    sgf_write::write_export_analyzed(rec, &Export::RawWithComments { comment: &comment }, Some(a))
                })
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
        let rec = &self.doc.record;
        let labels = comment_labels(self.active_language);
        let comment = |id| {
            (self.config.record.append_winrate_to_comment && id != ROOT && rec.node(id).kind != NodeKind::Snapshot)
                .then(|| self.formatted_comment(id, &labels))
                .flatten()
        };
        self.write_payloads(|a| sgf_write::write_analyzed(rec, application, a, &comment))
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
