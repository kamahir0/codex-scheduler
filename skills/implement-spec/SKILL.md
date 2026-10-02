---
name: implement-spec
description: Implement and verify behavior from Approved specifications, maintaining recoverable Git/state checkpoints without inventing observable semantics.
---

# implement-spec

## 目的

Approved specificationを実装し、self-reviewとrequired validationを終えたfinal candidateへ到達する。

authority / Human gate / recovery / Git deliveryは`AGENTS.md`と[Execution Workflow](../../docs/execution-workflow.md)、documentation quantityは[Documentation Policy](../../docs/contributing/documentation-policy.md)がownerであり、このskillへ共通policyを複製しない。

targetが`Approved`でなければproduct codeを変更しない。

## Context loading

fresh-session recoveryはExecution Workflowに従う。その上で次だけを読む。

1. Current Objective / Development State / Work base / Active work。
2. target Approved / Implemented specと直接関係するADR。
3. Work baseからcurrent HEADのGit reality。
4. affected code / tests / fixtures / adapters / relevant CI。

Applied spec-change artifactはhistorical auditでありimplementation authorityにしない。

## Work package

実装前にtask-localで次をrecoverする。

- Objective / canonical authority
- completion slice / explicit non-scope
- invariant / failure / compatibility expectation
- affected implementation boundary
- regression evidence / validation command

stable expectationがcanonical specにあるなら別documentへ複製しない。Requirement IDを必要な範囲でtest / owner code boundaryへ対応付ける。

## Implementation

Approved acceptanceを満たす最小で明瞭な変更を行う。adapter都合でdomain semanticsを複製しない。

test可能なchanged behaviorにはfocused regression evidenceを置く。fixtureはstable end-to-end input自体に価値がある場合だけ使う。

Approved authorityからobservable behaviorを安全に決められない場合はimplementation codeで補完せずSpecification Gapとしてspecification workflowへ戻す。Human gateがなければ同じrunでreview/approval/application後にimplementationへ復帰できる。

## Recovery checkpoints

long-running workでは[Coherent implementation checkpoints](../../docs/execution-workflow.md#coherent-implementation-checkpoints)を使う。

meaningful sliceがfocused validationを通ったらcheckpoint commitを作ってよい。commit後、Development StateのActive workをCompleted / In progress / Remainingのwork package / Requirement IDだけで更新してよい。

毎file / 毎testのprogress logは残さない。Git/code/testsがimplementation realityであり、stateはresume pointerに留める。

## Local rationale

non-obviousなworkaround、ordering/concurrency、platform-specific behavior、intentional redundancy、optimization、unusual filesystem/error handling等だけ、[Implementation Rationale](../../docs/contributing/implementation-rationale.md)に従ってnearby WHYを残す。

既存specやtestから理由を十分復元できるなら追加commentを書かない。rationale-sensitiveな変更では鮮度を再確認し、必要なら更新 / 削除する。

## Final candidate protocol

```text
authority / recovery
  -> implementation
  -> focused regression evidence
  -> rationale freshness when relevant
  -> review-code self-review
  -> safe self-fix
  -> affected validation
  -> cargo xtask check-rationale
  -> cargo xtask check-all
  -> diff / scope review
  -> candidate commit
  -> verification-ready
  -> fresh verification
```

環境が対応する場合は`cargo xtask check-all`を最終repository checkに使う。実行不能checkは理由を報告し、完全なverificationを主張しない。

## Specification status

`Approved -> Implemented`は、そのspec scopeのacceptance evidenceが揃い、tests/fixtures/compatibility evidenceが同期し、required checksが成功または実行不能理由が明示され、未解決Specification Gapがない場合だけ行う。

implementationに合わせてnormative languageを弱めたり、unapproved behaviorをauthorityへ昇格させたりしない。

## Specification Gap report

```text
Specification Gap
- Spec ID / file:
- Missing decision:
- Why implementation cannot proceed safely:
- Non-semantic work that can proceed:
- Proposed route:
```

Human gate判定はExecution Workflow、Agent Decision / approvalはSpecification Workflowへ委ねる。

## Completion report

- Objective / authority / non-scope
- implementation boundary / compatibility impact
- acceptance evidence / self-review / required checks
- Candidate SHA / push result
- unresolved Gap / unavailable check

既知のcanonical semanticsやacceptance matrixをreportへ再複製しない。
