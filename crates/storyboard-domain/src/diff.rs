use crate::ids::VersionNumber;
use serde::{Deserialize, Serialize};

/// Structured diff between two project versions. Category tags let the UI
/// group changes by 身份 / 场景 / 其他 (plan §20 Library Diff page).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDiff {
    pub from_version: VersionNumber,
    pub to_version: VersionNumber,
    pub global: Vec<FieldChange>,
    pub panels: Vec<PanelDiff>,
    pub summary: DiffSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub panels_added: u32,
    pub panels_removed: u32,
    pub panels_modified: u32,
    pub prompt_chars_added: u64,
    pub prompt_chars_removed: u64,
    /// 1 - (changed chars / total chars) over all panel prompts.
    pub preservation_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub path: String,
    pub category: ChangeCategory,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCategory {
    Identity,
    Scene,
    Structure,
    Params,
    Meta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelDiff {
    /// Panel index in the *new* version; None for removed panels.
    pub index: Option<u32>,
    pub old_index: Option<u32>,
    pub panel_id: Option<String>,
    pub changes: Vec<PanelChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelChange {
    Added,
    Removed,
    Prompt { before: String, after: String, tokens: TokenDiff },
    CharacterSlot { slot: u32, before: String, after: String, tokens: TokenDiff },
    Field { field: String, before: serde_json::Value, after: serde_json::Value },
}

/// Comma-token level diff computed with LCS. Deterministic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenDiff {
    pub changes: Vec<TokenChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenChange {
    Kept { token: String },
    Removed { token: String },
    Added { token: String },
}

fn split_tokens(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

impl TokenDiff {
    pub fn compute(before: &str, after: &str) -> TokenDiff {
        let a = split_tokens(before);
        let b = split_tokens(after);
        // LCS table (prompts are a few hundred tokens; fine).
        let n = a.len();
        let m = b.len();
        let mut dp = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i][j] = if a[i] == b[j] {
                    dp[i + 1][j + 1] + 1
                } else {
                    dp[i + 1][j].max(dp[i][j + 1])
                };
            }
        }
        let mut changes = Vec::new();
        let (mut i, mut j) = (0usize, 0usize);
        while i < n && j < m {
            if a[i] == b[j] {
                changes.push(TokenChange::Kept { token: a[i].clone() });
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                changes.push(TokenChange::Removed { token: a[i].clone() });
                i += 1;
            } else {
                changes.push(TokenChange::Added { token: b[j].clone() });
                j += 1;
            }
        }
        while i < n {
            changes.push(TokenChange::Removed { token: a[i].clone() });
            i += 1;
        }
        while j < m {
            changes.push(TokenChange::Added { token: b[j].clone() });
            j += 1;
        }
        TokenDiff { changes }
    }

    pub fn kept(&self) -> usize {
        self.changes.iter().filter(|c| matches!(c, TokenChange::Kept { .. })).count()
    }
    pub fn removed(&self) -> usize {
        self.changes.iter().filter(|c| matches!(c, TokenChange::Removed { .. })).count()
    }
    pub fn added(&self) -> usize {
        self.changes.iter().filter(|c| matches!(c, TokenChange::Added { .. })).count()
    }
}
