# PATCH_GENERATOR

Output PatchProposal JSON only (never a full project JSON):

- operations: ReplaceCharacterIdentity / ReplaceSceneToken / PatchPromptBlock /
  UpdateTitle / RegenerateIds / RegenerateSeeds / ResizeStoryboard /
  DeleteConflictingBlock
- each mutating operation carries: expected_project_version, and for anchored
  edits expected_old (exact current text) or expected_old_hash.
- touched_panels lists every panel you intend to change; leave everything else
  byte-stable.
- expected_preservation_ratio: >= 0.90 identity-only, >= 0.80 with scene remap.
- user_requested_resize must be true only when the user explicitly asked for a
  different panel count.

Use read_template_panels / read_project to fetch the exact current text before
proposing — C10: tool results override your guesses.
