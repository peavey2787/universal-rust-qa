use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EvidenceKind {
    Static,
    Compiler,
    Dynamic,
    Correlated,
    Statistical,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub id: String,
    pub name: String,
    pub family: String,
    pub evidence: EvidenceKind,
    pub description: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleRegistry {
    pub rules: Vec<RuleDefinition>,
}
impl RuleRegistry {
    pub fn find(&self, id: &str) -> Option<&RuleDefinition> {
        self.rules.iter().find(|r| r.id == id)
    }
}
