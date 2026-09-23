Read the failing CI logs with `gh run view --log-failed` and classify the failure.
Classify as one of: flaky (test infrastructure or race condition, not a code bug), real (a genuine defect introduced by this PR), or unrelated (a failure on main that is not caused by this PR).
Provide concrete evidence: the exact test name, log excerpt, and your reasoning.
For a real failure describe the change that caused it and sketch a fix.
Report findings in report.md.
Do not modify code, push commits, or rerun checks.
