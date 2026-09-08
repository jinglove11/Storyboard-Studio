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
        let has = |t: &str| {
            ops.map(|o| o.iter().any(|x| x["type"] == t))
                .unwrap_or(false)
        };
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
/// compacted to system + first user + a tail window. BOTH limits are
/// enforced — a few oversized tool results must not silently bypass the
/// message-count ceiling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ContextBudget {
    pub max_messages: usize,
    pub max_chars: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_messages: 40,
            max_chars: 24_000,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionStats {
    pub removed_messages: usize,
    pub removed_chars: usize,
}

fn message_weight(m: &model_providers::ChatMessage) -> usize {
    m.content.len()
        + m.tool_calls
            .iter()
            .map(|t| t.arguments_json.len())
            .sum::<usize>()
        + 32
}

/// Compact `msgs` in place: keep [0]=system, [1]=first user, and the largest
/// tail window that fits both the message-count and the char budget (floor:
/// 3 tail messages so a complete tool round can survive). One marker message
/// replaces the dropped middle. Afterwards `repair_tool_pairing` removes any
/// assistant/tool message split by the cut — providers reject dangling tool
/// results and hang on unanswered tool calls.
pub fn apply_context_budget(
    msgs: &mut Vec<model_providers::ChatMessage>,
    budget: &ContextBudget,
) -> Option<CompactionStats> {
    let total_chars: usize = msgs.iter().map(message_weight).sum();
    let over_messages = msgs.len() > budget.max_messages;
    let over_chars = total_chars > budget.max_chars;
    if (!over_messages && !over_chars) || msgs.len() <= 5 {
        return None;
    }

    // start from the message-budget window and shrink for the char budget
    let mut keep = (budget.max_messages.saturating_sub(3))
        .min(msgs.len() - 2)
        .max(3);
    while keep > 3 {
        let tail: usize = msgs[msgs.len() - keep..].iter().map(message_weight).sum();
        if message_weight(&msgs[0]) + message_weight(&msgs[1]) + tail + 64 <= budget.max_chars {
            break;
        }
        keep -= 1;
    }
    let split = msgs.len() - keep;
    if split <= 2 {
        return None; // nothing more we can drop
    }
    let removed: Vec<model_providers::ChatMessage> = msgs.splice(2..split, Vec::new()).collect();
    let stats = CompactionStats {
        removed_messages: removed.len(),
        removed_chars: removed
            .iter()
            .map(message_weight)
            .sum::<usize>()
            .saturating_sub(removed.len() * 32),
    };
    msgs.insert(
        2,
        model_providers::ChatMessage::system(format!(
            "[context compacted: {} earlier message(s) omitted ({} chars)]",
            stats.removed_messages, stats.removed_chars
        )),
    );
    repair_tool_pairing(msgs);
    Some(stats)
}

/// Fixpoint pass: drop tool results whose calling assistant is gone, and drop
/// assistant tool_calls whose results are gone. Pairing is by `tool_call_id`,
/// never by message adjacency.
pub fn repair_tool_pairing(msgs: &mut Vec<model_providers::ChatMessage>) {
    use std::collections::BTreeSet;
    loop {
        let callers: BTreeSet<String> = msgs
            .iter()
            .filter(|m| !m.tool_calls.is_empty())
            .flat_map(|m| m.tool_calls.iter().map(|t| t.id.clone()))
            .collect();
        let results: BTreeSet<String> = msgs
            .iter()
            .filter(|m| matches!(m.role, model_providers::Role::Tool))
            .filter_map(|m| m.tool_call_id.clone())
            .collect();
        let mut changed = false;
        let mut i = 0;
        while i < msgs.len() {
            let drop_here = if matches!(msgs[i].role, model_providers::Role::Tool) {
                let id = msgs[i].tool_call_id.clone().unwrap_or_default();
                !callers.contains(&id)
            } else if !msgs[i].tool_calls.is_empty() {
                // an assistant whose tool results were dropped must go too —
                // otherwise the model re-issues the call with stale arguments
                !msgs[i].tool_calls.iter().all(|t| results.contains(&t.id))
            } else {
                false
            };
            if drop_here {
                msgs.remove(i);
                changed = true;
            } else {
                i += 1;
            }
        }
        if !changed {
            break;
        }
    }
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
            approval: ApprovalPolicy {
                mode: ApprovalMode::AlwaysPrompt,
            },
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
    Completed {
        reply: String,
    },
    NeedsApproval {
        patch_id: i64,
        auto_approved: bool,
        risk: String,
    },
    ValidationExhausted {
        failures: Vec<String>,
    },
    Cancelled,
    Failed {
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use model_providers::ChatMessage;

    #[test]
    fn budget_compacts_middle_only() {
        let budget = ContextBudget {
            max_messages: 6,
            max_chars: 1_000_000,
        };
        let mut msgs = vec![ChatMessage::system("sys"), ChatMessage::user("first")];
        for i in 0..20 {
            msgs.push(ChatMessage::user(format!("m{i}")));
        }
        let stats = apply_context_budget(&mut msgs, &budget).unwrap();
        assert_eq!(stats.removed_messages, 17);
        // final length must respect the budget (the old implementation kept
        // max_messages+1 — an off-by-one that let history creep upward)
        assert!(
            msgs.len() <= budget.max_messages,
            "len {} > budget {}",
            msgs.len(),
            budget.max_messages
        );
        assert_eq!(msgs[0].content, "sys");
        assert_eq!(msgs[1].content, "first");
        assert_eq!(msgs.last().unwrap().content, "m19");

        let mut small = vec![ChatMessage::system("s"), ChatMessage::user("u")];
        assert!(apply_context_budget(&mut small, &budget).is_none());
    }

    /// A handful of oversized messages must trigger compaction even when the
    /// message count is under the ceiling (the old check ignored max_chars).
    #[test]
    fn budget_enforces_char_ceiling() {
        let budget = ContextBudget {
            max_messages: 100,
            max_chars: 2_000,
        };
        let mut msgs = vec![ChatMessage::system("sys"), ChatMessage::user("first")];
        for i in 0..30 {
            msgs.push(ChatMessage::user(format!("m{i}: {}", "x".repeat(200))));
        }
        let stats = apply_context_budget(&mut msgs, &budget).unwrap();
        assert!(stats.removed_messages > 0);
        let total: usize = msgs.iter().map(|m| m.content.len() + 32).sum();
        assert!(
            total <= 2_000 + 200,
            "post-compaction weight {total} exceeds budget"
        );
        assert_eq!(msgs[0].content, "sys");
        assert!(msgs.last().unwrap().content.contains("m29"));
    }

    /// Tool results pair with their assistant by id, never by adjacency:
    /// after compaction no dangling tool message and no unanswered call.
    #[test]
    fn budget_repairs_tool_pairing() {
        let budget = ContextBudget {
            max_messages: 6,
            max_chars: 1_000_000,
        };
        let call = |id: &str| ChatMessage {
            role: model_providers::Role::Assistant,
            content: String::new(),
            tool_calls: vec![model_providers::ToolCall {
                id: id.into(),
                name: "read_project".into(),
                arguments_json: "{}".into(),
            }],
            tool_call_id: None,
        };
        let mut msgs = vec![ChatMessage::system("sys"), ChatMessage::user("first")];
        msgs.push(call("c1"));
        msgs.push(ChatMessage::tool_result("c1", "r1"));
        for i in 0..20 {
            msgs.push(ChatMessage::user(format!("m{i}")));
        }
        msgs.push(call("c2"));
        msgs.push(ChatMessage::tool_result("c2", "r2"));
        let stats = apply_context_budget(&mut msgs, &budget).unwrap();
        assert!(stats.removed_messages > 0);
        // tail pair (c2) survives intact
        assert!(msgs
            .iter()
            .any(|m| m.tool_calls.iter().any(|t| t.id == "c2")));
        assert!(msgs
            .iter()
            .any(|m| matches!(m.role, model_providers::Role::Tool)
                && m.tool_call_id.as_deref() == Some("c2")));
        // no dangling tool results / unanswered calls anywhere
        use std::collections::BTreeSet;
        let callers: BTreeSet<String> = msgs
            .iter()
            .flat_map(|m| m.tool_calls.iter().map(|t| t.id.clone()))
            .collect();
        let results: BTreeSet<String> = msgs
            .iter()
            .filter(|m| matches!(m.role, model_providers::Role::Tool))
            .filter_map(|m| m.tool_call_id.clone())
            .collect();
        for m in msgs
            .iter()
            .filter(|m| matches!(m.role, model_providers::Role::Tool))
        {
            assert!(
                callers.contains(m.tool_call_id.as_deref().unwrap_or("")),
                "dangling tool result"
            );
        }
        for m in msgs.iter().filter(|m| !m.tool_calls.is_empty()) {
            assert!(
                m.tool_calls.iter().all(|t| results.contains(&t.id)),
                "unanswered tool call"
            );
        }
    }

    /// Adjacent-but-unrelated pairs: a tool result whose calling assistant
    /// was cut by compaction must be dropped even though the NEXT message is
    /// an unrelated user turn (the old adjacency heuristic kept it).
    #[test]
    fn budget_drops_orphaned_tool_result_before_user_msg() {
        let mut msgs = vec![
            ChatMessage::system("s"),
            ChatMessage::user("first"),
            // (the assistant that issued call cX was removed by compaction)
            ChatMessage::tool_result("cX", "orphan-risk"),
            ChatMessage::user("next"),
        ];
        crate::turn::repair_tool_pairing(&mut msgs);
        assert!(
            !msgs.iter().any(|m| m.content == "orphan-risk"),
            "orphaned tool result must be dropped"
        );
        assert!(
            msgs.iter().any(|m| m.content == "next"),
            "unrelated user turn must survive"
        );
        // an assistant whose tool result is missing must be dropped too
        let mut msgs2 = vec![
            ChatMessage::system("s"),
            ChatMessage::user("first"),
            ChatMessage {
                role: model_providers::Role::Assistant,
                content: String::new(),
                tool_calls: vec![model_providers::ToolCall {
                    id: "cY".into(),
                    name: "t".into(),
                    arguments_json: "{}".into(),
                }],
                tool_call_id: None,
            },
            ChatMessage::user("after"),
        ];
        crate::turn::repair_tool_pairing(&mut msgs2);
        assert!(
            !msgs2.iter().any(|m| !m.tool_calls.is_empty()),
            "assistant with unanswered tool call must be dropped"
        );
    }

    #[test]
    fn approval_risk_levels() {
        let p = ApprovalPolicy {
            mode: ApprovalMode::AutoLowRisk,
        };
        let low = serde_json::json!({"operations": [{"type": "replace_character_identity"}]});
        assert_eq!(p.decide(&low), (true, "low"));
        let high = serde_json::json!({"operations": [{"type": "resize_storyboard"}]});
        assert_eq!(p.decide(&high), (false, "high"));
    }
}
