//! Turn configuration types: approval policy, runtime config, context budget.

use model_providers::SamplingParams;
use serde::{Deserialize, Serialize};
use storyboard_tools::AgentProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalMode {
    /// Auto-approve low-risk patches (identity/scene), prompt otherwise.
    AutoLowRisk,
    /// Always ask the user.
    AlwaysPrompt,
}

#[derive(Debug, Clone)]
pub struct ApprovalPolicy {
    pub mode: ApprovalMode,
}

impl ApprovalPolicy {
    /// Decide for a proposal; returns (approved, risk).
    pub fn decide(&self, proposal: &serde_json::Value) -> (bool, &'static str) {
        let ops = proposal["operations"].as_array();
        let has = |t: &str| ops.map(|o| o.iter().any(|x| x["type"] == t)).unwrap_or(false);
        let risk: &'static str = if has("resize_storyboard") {
            "high"
        } else if has("delete_conflicting_block") {
            "medium"
        } else {
            "low"
        };
        let approved = matches!((self.mode, risk), (ApprovalMode::AutoLowRisk, "low"));
        (approved, risk)
    }
}

/// Context budget (plan §29): before every model call the conversation is
/// compacted to system + first user + a tail window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ContextBudget {
    pub max_messages: usize,
    pub max_chars: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self { max_messages: 40, max_chars: 24_000 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionStats {
    pub removed_messages: usize,
    pub removed_chars: usize,
}

/// Compact `msgs` in place: keep [0]=system, [1]=first user, the tail window,
/// and one marker message replacing the dropped middle. Dangling tool
/// results at the seam are removed (their calls are gone).
pub fn apply_context_budget(
    msgs: &mut Vec<model_providers::ChatMessage>,
    budget: &ContextBudget,
) -> Option<CompactionStats> {
    if msgs.len() <= budget.max_messages {
        return None;
    }
    let keep_tail = budget.max_messages.saturating_sub(2);
    let split = msgs.len() - keep_tail;
    if split <= 2 {
        return None;
    }
    let removed: Vec<model_providers::ChatMessage> = msgs.splice(2..split, Vec::new()).collect();
    let stats = CompactionStats {
        removed_messages: removed.len(),
        removed_chars: removed.iter().map(|m| m.content.len()).sum(),
    };
    msgs.insert(
        2,
        model_providers::ChatMessage::system(format!(
            "[context compacted: {} earlier message(s) omitted ({} chars)]",
            stats.removed_messages, stats.removed_chars
        )),
    );
    // tool results without their assistant tool_calls confuse providers
    let mut i = 2;
    while i + 1 < msgs.len() {
        let cur_tool = matches!(msgs[i].role, model_providers::Role::Tool);
        let next_is_caller = !msgs[i + 1].tool_calls.is_empty();
        if cur_tool && !next_is_caller {
            // a tool message followed by a plain message: it belongs to a
            // removed assistant turn — drop it
            let _ = msgs.remove(i);
            continue;
        }
        i += 1;
    }
    Some(stats)
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub profile: AgentProfile,
    pub approval: ApprovalPolicy,
    pub max_tool_rounds: usize,
    pub max_validator_retries: usize,
    pub sampling: SamplingParams,
    pub budget: ContextBudget,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            profile: AgentProfile::StoryboardProduction,
            approval: ApprovalPolicy { mode: ApprovalMode::AlwaysPrompt },
            max_tool_rounds: 8,
            max_validator_retries: 2,
            sampling: SamplingParams::default(),
            budget: ContextBudget::default(),
        }
    }
}

/// Terminal outcome of one submitted user turn.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnStatus {
    Completed { reply: String },
    NeedsApproval { patch_id: i64, auto_approved: bool, risk: String },
    ValidationExhausted { failures: Vec<String> },
    Cancelled,
    Failed { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use model_providers::ChatMessage;

    #[test]
    fn budget_compacts_middle_only() {
        let budget = ContextBudget { max_messages: 6, max_chars: 1_000_000 };
        let mut msgs = vec![ChatMessage::system("sys"), ChatMessage::user("first")];
        for i in 0..20 {
            msgs.push(ChatMessage::user(format!("m{i}")));
        }
        let stats = apply_context_budget(&mut msgs, &budget).unwrap();
        assert_eq!(stats.removed_messages, 16);
        assert_eq!(msgs[0].content, "sys");
        assert_eq!(msgs[1].content, "first");
        assert_eq!(msgs.last().unwrap().content, "m19");

        let mut small = vec![ChatMessage::system("s"), ChatMessage::user("u")];
        assert!(apply_context_budget(&mut small, &budget).is_none());
    }

    #[test]
    fn approval_risk_levels() {
        let p = ApprovalPolicy { mode: ApprovalMode::AutoLowRisk };
        let low = serde_json::json!({"operations": [{"type": "replace_character_identity"}]});
        assert_eq!(p.decide(&low), (true, "low"));
        let high = serde_json::json!({"operations": [{"type": "resize_storyboard"}]});
        assert_eq!(p.decide(&high), (false, "high"));
    }
}
