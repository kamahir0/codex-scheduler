# 実装理由とReverse Traceability

Workflow status: Active

このガイドは**local implementation WHY**だけを扱う。observable product behavior、architecture decision、test evidence、Current Objective等のowner routingは[Documentation Policy](documentation-policy.md)が定義する。

## いつrationaleを書くか

straightforwardな実装から意図的に外れ、理由を知らないfuture maintainerが合理的だが危険なsimplificationをし得る場合だけnearby rationaleを残す（MUST）。

典型例:

- regression回避
- platform / library / toolchain workaround
- ordering / timing / concurrency constraint
- intentional state duplication / copy
- unusual filesystem / error handling
- measured performance optimization
- temporary workaround with removal condition

obvious private helper、通常のbranch、clone、allocation等にはcommentを要求しない。

既存spec、test、ADR、code structureだけでWHYとfailure modeを十分復元できる場合、追加rationaleを書かない。

## Rationale content

必要な範囲で次を復元できること。

- WHY: なぜstraightforwardな形にしないのか。
- WHAT BREAKS: 削除 / 簡略化すると何が壊れるか。
- EVIDENCE: focused test、Requirement ID、ADR、benchmark、external constraint等。
- REMOVAL CONDITION: temporary workaroundで必要な場合。

referenceだけ（例: `See issue #123`）にせず、referenceを読めなくても最低限のprotected invariantが分かるようにする。

## Rationale freshness

rationaleの近くにあるimplementation、protected invariant、referenced test / Requirement / ADR、dependency / platform assumptionを変更した場合は同じchangeでrationaleを再検証する（MUST）。

結果は次のいずれか。

- `Still accurate`: current implementation / failure mode / evidenceと一致。
- `Invariant/reason changed`: rationaleとevidenceを更新。
- `Reason no longer applies`: protected invariantが消えた、または別evidenceで十分に守られることを確認して削除。

test成功だけではrationale freshnessを証明しない。

## Regression fixes

Approved specificationがobservable behaviorを定義していてimplementationだけが違反する場合、specを変更せずbug fix + focused regression testで直す。non-obvious implementation shapeが残る時だけlocal rationaleを補う。

Approved specからbehaviorを選択できない場合はSpecification Gapであり、commentで新product ruleを作らない。

## Refactor / simplification

unusual codeをsimplify、delete、deduplicate、replaceする前に、discoverableなnearby rationale、Requirement ID、regression test、ADR、benchmark、platform constraintを確認する（MUST）。

refactorでprotected codeを移動する場合、必要なrationaleもinvariantの近くへ移す。理由が不要になったならobsolete commentを残さない。

## Performance rationale

「faster」だけを書かない。可能ならbenchmark / profile / allocation evidence / known hot pathへtraceする。observable contractを変更しないoptimizationをspecへ追加しない。

## Structural reference check

`cargo xtask check-rationale`はcomment内の明示referenceについて、Requirement ID、ADR/RFC、Regression test identifier、repository-relative docs pathの存在を確認する。

このcheckは自然言語のWHY、failure mode、workaround必要性、benchmark妥当性を判定しない。semantic freshnessはreview-codeで判断する。

## Example

```rust
// WHY: keep the previous output until the new build is complete.
// IF REMOVED: a failed build can destroy the last valid artifact.
// EVIDENCE: failed_build_preserves_previous_output.
// REMOVE WHEN: the filesystem adapter provides the same atomic guarantee.
```

この形式は例でありcomment DSLではない。
