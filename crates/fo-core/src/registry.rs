use crate::tool::Tool;
use fo_types::{FileKind, Platform, ToolInfo};
use std::sync::Arc;

#[derive(Default)]
pub struct Registry {
    tools: Vec<Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Tools that accept `kind` on `platform`, sorted by label.
    pub fn tools_for(&self, kind: &FileKind, platform: Platform) -> Vec<ToolInfo> {
        let mut v: Vec<ToolInfo> = self
            .tools
            .iter()
            .filter(|t| t.accepts(kind))
            .map(|t| t.info_for(kind))
            .filter(|i| i.platforms.contains(&platform))
            .collect();
        v.sort_by(|a, b| a.label.cmp(&b.label));
        v
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.info().id == id).cloned()
    }
}
