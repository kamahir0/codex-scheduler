---
name: refine-spec
description: Turn a request or Current Objective into a traceable Draft/Proposed specification change, resolving Objective-local decisions only under the canonical specification workflow.
---

# refine-spec

## 目的

product/domain/compatibility/user-visible GUI behaviorを変更するrequestを、review可能なDraft/Proposed changeへ変換する。

Human gate、Agent Decision eligibility、autonomous approvalは[Specification Workflow](../../docs/contributing/specification-workflow.md)と[Execution Workflow](../../docs/execution-workflow.md#human-gate)がownerであり、このskillへcriteriaを複製しない。documentation retentionは[Documentation Policy](../../docs/contributing/documentation-policy.md)に従う。

このskillはproduct implementationを行わず、authoring pass中に`Approved`へstatus transitionしない。

## Context

- Current Objective / Development State
- affected Approved / Implemented specification
- related ADR / RFC / terminology
- existing Requirement ID / related wording
- affected implementation / tests / fixturesはimpact evidenceとして必要な範囲

memory、stale conversation、current codeだけからproduct ruleを作らない。

## Procedure

### 1. Evidenceを分類する

必要なstatementを`Decision`、`Requirement`、`Constraint`、`Preference`、`Proposal`、`Idea`、`Question`、`Open Question`、`Rejected`、`Agent Decision`へ分類する。

Agent DecisionはHuman発言と明確に区別し、Specification Workflowの条件を満たす場合だけ使う。

### 2. Canonical ownerを特定する

同じsemantic ruleを複数documentへcopyしない。Approved / Implemented canonical documentへのsemantic deltaは`docs/spec-changes/`の別artifactへ置く。Accepted RFCやcurrent codeはimplementation authorityではない。

new Requirement IDは既存definitionを検索してから割り当て、再利用しない。Requirement IDとruntime Diagnostic Codeを混同しない。

### 3. Normative strengthを保持する

Human evidenceのcertaintyを強めない。Agent DecisionをHuman requirementとして偽装しない。

- `MUST / MUST NOT`: required invariant / constraint
- `SHOULD / SHOULD NOT`: strong default with exception rationale
- `MAY`: permission / capability

### 4. Open Questionをrouteする

- Agent-resolvable -> alternativesとreasonを記録しAgent Decisionへ。
- Human gate -> unresolvedのままHuman decision neededへ。
- Objective外 -> explicit non-scope / deferred。
- evidence不足 -> Specification / Knowledge Gap。

implementation convenienceのために黙って解決しない。

### 5. Downstream impactを記録する

compatibility、affected boundary、focused acceptance evidence、fixture needを、missing semanticsを発明せず記述する。architecture WHYがcross-cuttingならADR候補、large alternative比較ならRFC候補へrouteする。

## Required proposal content

- Affected Specifications
- Source Evidence and Classification
- Confirmed Decisions
- New / Changed Requirements
- Open Questions
- Potential ADRs
- Compatibility Impact
- Implementation Impact
- Approval Eligibility

Approval Eligibilityには`Autonomous approval eligible: Yes|No`と`Human gate: None|<reason>`を置く。判定criteriaはcanonical workflowを参照する。

## Status

整理中は`Draft`、review可能なら`Proposed`。refine-spec pass自身は`Approved`へ変更しない。

## Safety

- conversation logをspecへ貼らない。
- current implementationを未承認requirementへ昇格しない。
- one knowledge, one ownerを守る。
- future maintainerのためという理由だけで重複proseを増やさない。
