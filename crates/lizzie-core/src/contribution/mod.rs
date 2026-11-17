//! 跑谱贡献 (KataGo distributed training) pure logic ported from the fixed Java build
//! (`analysis/ContributeEngine.java`, `util/Utils.getRuleString`,
//! `rules/SGFParser.formatCommentPk`, `util/SavedEnginePathResolver.java`): the command line,
//! output classification and tips, the JSON game stream, the PK comment and saved-engine
//! candidates. Nothing here touches the document, engines or settings; the workbench flow owns
//! that.

pub mod command;
pub mod comment;
pub mod output;
pub mod rules;
pub mod saved;
pub mod snapshot;
