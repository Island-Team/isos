# R03 IsOS control-grant handoff cases

[中文](README_cn.md) · [IsOS owner contract](../../../docs/isos/interfaces/control-grants.md) · [Design](../../../docs/isos/design/control-grants.md)

`cases.json` is the frozen design matrix for IsOS issuance and launch delivery. It is not a passing-runtime claim. A clean `ISOS_HOME`, a verified schema-2 graph generation, a selected EOS stub, an optional selected EGIA stub and an EOS bridge stub form the baseline. Each stub records only the *names* of reserved environment variables it sees. The EOS R03 suite independently checks verifier bytes, gRPC metadata, role/method scopes and Dispatcher release. No fixture writes a real grant into a source checkout.

Positive cases establish exact file format/mode, all three recipient paths, one verifier bound to saved revision and run ID, idempotent Start and automatic child restart reuse, same-revision explicit Restart rotation, and recovery after failed replacement using another new run. Negative cases inject unsafe files, malformed grant/verifier, changed revision, selected-recipient omission, missing P04 snapshot, an old `1.0.0` graph and a synthetic random-source failure. A rejection before replacement must leave active PIDs, revision and bundle intact. A failed replacement must terminate its candidate run; old credentials cannot authorize recovery. Stop retires the bundle after reaping recipients. The status, API error and current-run JSONL event must contain only sanitized code, run ID and revision, never a grant, its digest or bearer text.

Half-managed EOS launches with only the selected profile or only the verifier fail before listener bind. Neither input means standalone read-only, even if `--run-id` is present.

The implementation runner must map each `id` to a unit or clean-home boundary test, record the command, selected EOS/IsOS commits and result under ignored `logs/test/<run-id>/`, then obtain independent review. Run `python3 tests/fixtures/control_grants/check_cases.py` to validate the design matrix and bilingual links.
