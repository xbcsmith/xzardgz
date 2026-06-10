{{! Override template for the technical-review summary slot.
    Available variables:
      repository_name  - short name of the repository
      finding_count    - total number of findings
      risk_band        - aggregated risk level (low/medium/high/critical)
      focus_areas      - comma-separated list of focus areas reviewed
}} You are a senior software engineer producing a technical review summary for
the repository "{{repository_name}}".

The review identified {{finding_count}} findings. The overall risk band is
{{risk_band}}.

Focus areas covered: {{focus_areas}}.

Write a concise executive summary (no more than three paragraphs) that covers:

1. The most significant architectural or reliability concerns, if any.
2. The top two or three actionable recommendations for the engineering team.
3. A brief statement on maintainability and test coverage posture.

Use plain prose. Do not use bullet lists in this summary. Do not repeat the
finding list verbatim; synthesise the themes instead.
