/// Versioned prompt presets (plan §7). Loaded from the repo's `prompts/`
/// directory at compile time; future versions load from disk so packs can be
/// installed without rebuilding.
#[derive(Clone)]
pub struct PromptPresets {
    pub version: String,
    pub core_contract: String,
    pub intent_parser: String,
    pub template_match: String,
    pub character_replace: String,
    pub patch_generator: String,
    pub failure_recovery: String,
}

impl PromptPresets {
    pub fn v1() -> Self {
        Self {
            version: "v1".into(),
            core_contract: include_str!("../../../prompts/v1/CORE_CONTRACT.md").into(),
            intent_parser: include_str!("../../../prompts/v1/INTENT_PARSER.md").into(),
            template_match: include_str!("../../../prompts/v1/TEMPLATE_MATCH.md").into(),
            character_replace: include_str!("../../../prompts/v1/CHARACTER_REPLACE.md").into(),
            patch_generator: include_str!("../../../prompts/v1/PATCH_GENERATOR.md").into(),
            failure_recovery: include_str!("../../../prompts/v1/FAILURE_RECOVERY.md").into(),
        }
    }

    /// Load a preset directory from disk (installable prompt packs).
    pub fn from_dir(version: &str, dir: &std::path::Path) -> std::io::Result<Self> {
        let read = |name: &str| std::fs::read_to_string(dir.join(name));
        Ok(Self {
            version: version.into(),
            core_contract: read("CORE_CONTRACT.md")?,
            intent_parser: read("INTENT_PARSER.md")?,
            template_match: read("TEMPLATE_MATCH.md")?,
            character_replace: read("CHARACTER_REPLACE.md")?,
            patch_generator: read("PATCH_GENERATOR.md")?,
            failure_recovery: read("FAILURE_RECOVERY.md")?,
        })
    }

    /// Compose the system prompt for a turn. Kept small and modular — no
    /// single mega-prompt (plan §7).
    pub fn system_prompt(&self, task: &str) -> String {
        format!(
            "{}\n\n{}\n\n{}\n\n{}\n\n---\nTASK: {}",
            self.core_contract,
            self.intent_parser,
            self.template_match,
            self.patch_generator,
            task
        )
    }
}
