# Kamish research workspace

Research update: **31 October 2026**. Implementation reviewed at commit
`4ebfb74375ed2c6ba84e0fdbb4a0d864bb5cc86a`.

## Start here

**Implementation progress:** [Loophole closure ledger](LOOPHOLE_CLOSURE.md) tracks
the subsequent fixes, safe containment, format migrations and remaining work.
The research reports below retain their original revision-specific findings.

**Phase 0 update, 1 November 2026:** [Contract proposal](../docs/CONTRACT.md) and
[ADR-0015](../decisions.md#adr-0015-phase-0-contract-and-approval-gates) await
approval. The [fresh baseline](../audit/phase0_baseline_2026-10-31_204424Z/REPORT.md)
at `77bb051` has 102 workspace tests passing, original probes 11 passing, and
extended probes four passing / two failing. No distributed work item is closed.

1. [Loopholes and design review](LOOPHOLES_REVIEW.md): implementation counterexamples, unresolved semantics, and corrections to the original research.
2. [Configuration and experiment roadmap](CONFIGURATION_ROADMAP.md): settings available today, proposed settings, architecture choices, and prioritized experiments.
3. [Sources and prior-art comparison](SOURCES_AND_PRIOR_ART.md): dated primary sources and a five-system comparison.
4. [Probe results and reproduction](PROBE_RESULTS.md): 61 application tests pass; the independent research harness exposes 10 counterexamples in 11 checks.

**Main conclusion:** the defensible hypothesis is one recoverable signed class
policy applied consistently across admission, storage, replication, replay,
recovery and ownership change, with scope-specific evidence. Pulsar already
enforces allowed clusters server-side, and ciphertext storage is existing prior
art. Current local tests do not establish the full hypothesis or a comparative
advantage; performance comparisons must follow the missing correctness work.

## Original research documents reviewed

| Document | Role | Assessment |
| --- | --- | --- |
| [Research dossier](02_Hydra_Stream_Research_Dossier.docx) | Research questions, literature seeds, failure models, NotebookLM brief | Strong framing; a research agenda rather than implementation evidence. Reviewed the DOCX paragraph/table text via its XML. |
| [Baseline components](Hydra%20Stream%20%E2%80%94%20Baseline%20Components.md) | Thirteen engineering prerequisites | Still useful as the baseline exit criteria. Several P0 components remain unimplemented or only modeled in-process. |
| [What we do differently](Hydra%20Stream%20%E2%80%94%20What%20We%20Do%20Differently.md) | Seven proposed differentiators | Updated the prior-art wording where current primary documentation contradicts broad claims. Features remain proposals until integration evidence exists. |
| [Project documentation HTML](Hydra%20Stream%20%E2%80%94%20Project%20Documentation.html) | Rendered v0.1 design snapshot | Shares the design structure and claims of the root [project.md](../project.md); it is not a separate validation source. |

The dossier, design, ADRs, and milestone reports describe related work from the same project. Repeating a claim across them does not independently validate it. The reports below distinguish **reproduced**, **source-inspected**, **proposed**, and **not yet measured** findings.

## Recommended first work package

Implement authenticated topic/class resolution and persist the effective record policy. Then fix per-stream replication cursors and recovery, atomic/recoverable producer sessions, and broker ownership fencing. Bring up a real multi-process testbed before testing adaptive routing gains.

Outstanding scope decisions: custom broker versus existing substrate; required wire compatibility; topic-level versus mixed-record classifications; target failure domains and RPO/RTO; whether encrypted external storage is permitted for the selected data class.
