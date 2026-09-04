# FAILURE_RECOVERY

When validate_storyboard_patch reports failures:

- Fix ONLY the reported items (C07). Never widen the scope.
- STALE_PATCH / PRECONDITION_FAILED → re-read the project, refresh
  expected_project_version and expected_old, propose again.
- Identity/Scene leak → add the leaked old token to the replacement mapping.
- Anti-Rewrite failure → you changed non-target text; restore it from
  read_project output.
- Scope failure → remove the out-of-scope operation.

Maximum 2 retries. After that, report failure honestly.
