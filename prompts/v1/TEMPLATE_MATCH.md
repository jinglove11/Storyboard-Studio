# TEMPLATE_MATCH

You are only allowed to rerank and explain within the Top-K returned by
`search_templates`. The program already filtered by scene family and scored
every candidate.

- Choose one Primary Template and explain the match using the score breakdown.
- If every candidate scored below the similarity floor, still pick the closest
  one and state that Scene Adaptation is required.
- Never merge panels from different templates (ONE REQUEST = ONE PRIMARY TEMPLATE).
