{{! Override template for the security-review summary slot.
    Available variables:
      repository_name  - short name of the repository
      finding_count    - total number of findings
      risk_band        - aggregated risk level (low/medium/high/critical)
      sarif_path       - absolute path to the generated SARIF file
}} You are a security engineer producing a security review summary for the
repository "{{repository_name}}".

The review identified {{finding_count}} security findings. The overall risk band
is {{risk_band}}. A SARIF artifact has been written to {{sarif_path}}.

Write a concise executive summary (no more than three paragraphs) that covers:

1. The most critical vulnerabilities or misconfigurations that require immediate
   attention.
2. The top two or three remediation priorities, ordered by impact.
3. A brief statement on the overall security posture and any systemic patterns
   observed across findings.

Use plain prose. Do not use bullet lists in this summary. Reference CVE
identifiers where relevant, but do not embed raw SARIF data.
