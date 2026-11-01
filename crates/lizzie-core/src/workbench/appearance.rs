use super::Workbench;
use crate::api::{ApiError, ErrorKind};
use crate::appearance::{self, AppearanceSettings, ThemeSettings};
use crate::settings;

fn io(error: std::io::Error) -> ApiError { ApiError::new(ErrorKind::Io, format!("saving theme failed: {error}")) }

impl Workbench {
    /// ConfigDialog2.java:6191-6286: default edits belong to ui, named edits to theme.txt.
    /// Persist both files before publishing the new projection; roll the theme back on failure.
    pub fn set_appearance_settings(&mut self, mut appearance: AppearanceSettings, mut theme: ThemeSettings) -> Result<bool, ApiError> {
        if !appearance::valid_name(&appearance.theme) || !theme.valid()
            || (!(12..=20).contains(&appearance.frame_font_size) && appearance.frame_font_size != self.config.appearance.frame_font_size) {
            return Err(ApiError::new(ErrorKind::InvalidArgument,"appearance settings"));
        }
        // Menu.java:745-769: entering Apple enables Morandi; entering classic disables Apple.
        if appearance.apple_style && !self.config.appearance.apple_style { appearance.morandi_colors = true; }
        if !appearance.morandi_colors && self.config.appearance.morandi_colors { appearance.apple_style = false; }
        if self.config.appearance == appearance && self.appearance_theme == theme { return Ok(false); }
        let images = appearance::capture_external_images(&self.storage, &mut appearance, &mut theme).map_err(io)?;
        let path = if appearance.theme == "default" { None } else { Some(appearance::theme_file(&self.storage,&appearance.theme).map_err(io)?) };
        let previous = if let Some(path) = &path {
            match std::fs::read(path) { Ok(bytes)=>Some(bytes),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,Err(e)=>return Err(io(e)) }
        } else { None };
        if let Some(path) = &path { settings::save_json(path,&appearance::to_java(&theme)).map_err(io)?; }
        let default = appearance.theme == "default";
        let result = self.commit_config(|c| {
            c.appearance = appearance;
            if default { c.appearance_defaults = theme.clone(); }
        });
        if let Err(error) = result {
            if let Some(path) = path {
                let restored = match previous {Some(bytes)=>settings::write_atomic(&path,&bytes),None=>std::fs::remove_file(path)};
                if let Err(restore) = restored { return Err(ApiError::new(ErrorKind::Io,format!("{error:?}; restoring theme failed: {restore}"))); }
            }
            return Err(error);
        }
        images.keep();
        self.appearance_theme = theme;
        Ok(true)
    }
    /// LizzieFrame.java:20741-20768: persisted now, active only on restart.
    pub fn set_frame_font_size(&mut self, size:i32) -> Result<bool,ApiError> {
        if !(12..=20).contains(&size) { return Err(ApiError::new(ErrorKind::InvalidArgument,"frame font size")); }
        if self.config.appearance.frame_font_size == size { return Ok(false); }
        self.commit_config(|c|c.appearance.frame_font_size=size)?;
        Ok(true)
    }
    /// ConfigDialog2.java:4391-4477: catalog management is immediate, independent of the
    /// appearance draft. Deletion never changes the running window's committed projection.
    pub fn add_theme(&mut self, name:&str) -> Result<(),ApiError> {
        appearance::add_theme(&self.storage,name).map_err(io)
    }
    pub fn delete_theme(&mut self, name:&str) -> Result<(),ApiError> {
        appearance::delete_theme(&self.storage,name).map_err(io)
    }
    pub(super) fn reload_appearance_theme(&mut self) {
        self.appearance_theme = appearance::load_theme(&self.storage,&self.config.appearance.theme,&self.config.appearance_defaults);
    }
    /// LizzieFrame.java:11429-11536, Board.java:6457-6473. All analysed branches use the
    /// same theme rules; an actual best move's negative changes count as zero. The mover's change
    /// is `lastWinrateScoreDiff` (`change_of`, shared with the 统计 / 超级鹰眼 evaluations).
    pub(super) fn theme_node_colors(&self) -> Vec<appearance::ThemeNodeColor> {
        if self.store.is_empty() { return Vec::new(); }
        use crate::record::NodeKind;
        let rec=&self.doc.record;
        let theme=&self.appearance_theme;
        rec.nodes().filter_map(|(node,n)| {
            let parent=n.parent?;
            if n.kind == NodeKind::DummyPass { return None; }
            let change=self.change_of(rec,&self.store,parent,node)?;
            let mut winrate=change.winrate;
            // Java scoreMean is 0 without a score.
            let mut score=change.score.unwrap_or(0.0);
            let before=self.store.get(rec,parent)?;
            let is_best=match n.kind {
                NodeKind::Move {point,..} => before.candidates.first().is_some_and(|candidate|candidate.point==Some(point)),
                _ => false,
            };
            if is_best && (winrate<0.0 || score<0.0) { winrate=0.0; score=0.0; }
            let diff=if theme.use_scorediff_in_variation_tree {
                let scaled=score*(1.0/theme.scorediff_in_variation_tree_factor);
                // Java Math.min propagates NaN (0 * infinity for a zero factor).
                if scaled.is_nan() { f64::NAN } else { winrate.min(scaled) }
            } else {winrate};
            let index=theme.blunder_winrate_thresholds.iter().position(|threshold|*threshold>=diff)?;
            Some(appearance::ThemeNodeColor {node,color:theme.blunder_node_colors[index].clone()})
        }).collect()
    }
}
