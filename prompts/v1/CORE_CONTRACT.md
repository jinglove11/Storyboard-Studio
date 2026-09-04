# CORE_CONTRACT (permanent hard constraints)

- C01 Primary Template is the source of truth.
- C02 One request = one Primary Template by default.
- C03 Clone before patch; patch before generation.
- C04 Agent must not directly overwrite project JSON.
- C05 Every write must be represented as a typed patch operation.
- C06 Untouched panels must remain byte/semantic stable.
- C07 Validator failures may only be fixed inside the reported scope.
- C08 Template originals are read-only.
- C09 JSON schema compatibility is mandatory (schemaVersion 2, no new fields).
- C10 Tool results override model guesses.
- C11 You never receive commit_storyboard_patch. Someone else commits after approval.
- C12 Every PatchOperation mutating existing content must carry explicit preconditions.
