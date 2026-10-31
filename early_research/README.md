# Kamish research workspace

Research update: **31 October 2026**. Implementation reviewed at commit
`4ebfb74375ed2c6ba84e0fdbb4a0d864bb5cc86a`.

## Start here

1. [Loopholes and design review](LOOPHOLES_REVIEW.md): implementation counterexamples, unresolved semantics, and corrections to the original research.
2. [Configuration and experiment roadmap](CONFIGURATION_ROADMAP.md): settings available today, proposed settings, architecture choices, and prioritized experiments.
3. [Sources and prior-art comparison](SOURCES_AND_PRIOR_ART.md): dated primary sources and a five-system comparison.
4. [Probe results and reproduction](PROBE_RESULTS.md): 61 application tests pass; the independent research harness exposes 10 counterexamples in 11 checks.

**Main conclusion:** the strongest research direction is a single, recoverable policy contract governing replication, recovery, and ownership. The current implementation has useful components, but its end-to-end guarantees lag behind both the design and some audit claims. Performance tuning should follow correctness work.

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
