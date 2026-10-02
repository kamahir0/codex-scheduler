---
name: review-spec
description: Independently challenge a Draft/Proposed specification change and report semantic readiness, autonomous approval eligibility, and any Human gate.
---

# review-spec

## 目的

refinementで作成されたproposalを、authoring assumptionsから離れたfresh challenge passとしてreviewする。

Human gate criteriaは[Execution Workflow](../../docs/execution-workflow.md#human-gate)、autonomous approvalは[Specification Workflow](../../docs/contributing/specification-workflow.md#autonomous-approval)、documentation ownershipは[Documentation Policy](../../docs/contributing/documentation-policy.md)がownerである。このskillへcriteria本文を複製しない。

review-spec pass自身はstatus transitionを行わない。

## Input

- target Draft / Proposed change
- Current Objective / Development State
- source request / durable evidence
- affected Approved / Implemented specs
- related ADR / RFC / terminology
- relevant tests / implementation evidence

source evidenceが取得できない場合、intent fidelityを推測しない。

## Review checklist

### Intent fidelity
- Human Decision / Requirement / Constraintのscopeとstrengthを保持しているか。
- Preference / IdeaをDecisionとして偽装していないか。
- Agent Decisionが明示され、canonical eligibilityを満たすか。
- Rejected / non-scopeを再導入していないか。

### Internal / cross-spec consistency
- status、requirements、validation、compatibility、Open Questionsが整合するか。
- Requirement IDがunique / stableか。
- duplicate ownerがないか。
- Approved contract / ADR boundaryと矛盾しないか。

### Normative strength
- MUST / SHOULD / MAYに根拠があるか。
- implementation detailをobservable contractへ固定していないか。

### Testability / failure semantics
- success / failure / conflict / compatibility outcomeをtest可能か。
- data safety、lost update、partial failure等の必要boundaryが定義されているか。
- focused regression evidenceを作れるか。

### Compatibility / Human gate
- compatibility impactが明示されているか。
- Human gate判定がExecution Workflowと一致するか。

### Documentation quality
- observable contract、architecture WHY、test evidence、local rationaleが正しいownerへrouteされているか。
- current checkoutへ残すproseがdecision recoverabilityに必要か。
-同じdecisionを複数ownerへcopyしていないか。

## Required output

### Blocking Issues
なければ`None identified`。

### Non-blocking Issues
なければ`None identified`。

### Questions
なければ`None identified`。

### Approved as Proposed
`Yes` / `No`。semantic review verdictでありstatus transitionではない。

### Autonomous approval eligibility
- `Eligible: Yes|No`
- `Human gate: None|<reason>`
- `Rationale: ...`

### Review dimensions
Intent fidelity、Internal consistency、Cross-spec consistency、Terminology、Normative strength、Testability、Backward compatibility、Unresolved ambiguity、Implementation leakage、Unrequested behavior、Documentation ownershipをcompactに示す。

## Handoff

- Approved=Yes + Eligible=Yes -> orchestrationはapprove/applyし、同じrunでimplementationへ進めてよい。
- Approved=Yes + Eligible=No -> `decision-required`へHuman gateを記録。
- Blockingあり -> refine-specへ戻す。Human gateがなければ同じrunで修正・再reviewしてよい。
- implementation違反 -> review-code / bug fixへroute。

minor editorial preferenceだけでHuman gateを作らない。
