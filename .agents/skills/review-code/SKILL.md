---
name: review-code
description: Review implementation diffs for specification conformance, regression safety, rationale freshness, reverse traceability, evidence integrity, and architecture boundaries.
---

# review-code

## 目的と責務

このskillは、実装diffが既存のauthorityとevidenceに照らして安全かを確認するimplementation reviewである。
`review-spec`が「仕様は正しいか、approval可能か」を確認するのに対し、`review-code`は「実装diffはその仕様に適合し、regressionとrationaleの鮮度が保たれているか」を確認する。

`review-code`はcanonical specificationを変更しない。Open Questionを解決せず、未定義のobservable behaviorを実装で発明しない。Approved specにbehaviorが定義されているのに実装が違反する場合はbug findingとして扱う。Approved specからbehaviorを選択しなければならない場合は`Specification Gap`として`refine-spec`へ戻す。

`review-code`はimplementation中のself-reviewと、Development Stateが`verification-ready`になったfinal verificationの両方で使用する。self-reviewではfirst draftをfinal candidateへ閉じるため、Approved authorityの範囲内で直せる`Blocking` findingを同じtask内で解消する。

final verificationは特定の`main-reviewer` roleを要求しない。同じagentが行う場合はfreshness gate後にDevelopment Stateのexact Candidate SHAを読み、implementation中の私的な意図やconversation上の自己評価をevidenceにせず、確定済みCandidate diffを別passとしてreviewする。risk / capability上有益なら別agentへdelegateしてよい。

## 必須のinput

変更範囲に応じて、次をdiff中心に読む。

- changed filesとimplementation diff
- `verification-ready` / `correction-ready`の場合はDevelopment Stateに記録されたexact Candidate SHA
- 関連する`Approved` / `Implemented` specificationとRequirement ID
- Current Objectiveのcompletion boundary / non-scope
- 関連test、fixture、goldenまたはbenchmark
- touched codeのnearby rationale comment
- 参照されたADR、RFC、issue/reference、external/platform constraint
- 必要なarchitecture boundary（core、application、CLI、GUI、codegen、.NET adapter）

repository全体を無制限に探索しない。変更から合理的にdiscoverableな関連範囲を確認する。ただし、unusual codeの削除・簡略化・移動では、nearby rationale、Requirement ID、regression test、ADR/RFC、benchmark、platform/library/toolchain constraintを検索してから判断する。

## Rationale-sensitiveな変更

次の変更はrationale freshnessを確認する。

- rationale commentに近接するcodeの変更
- rationaleが保護するfunction、method、moduleのrefactorまたは移動
- protected invariantの変更
- 参照されたregression testの変更または削除
- 参照されたRequirement IDのowner/status変更
- 参照されたADR/RFCのsupersede
- dependency、toolchain、platform assumptionの変更
- optimizationまたはworkaroundの変更
- simplify、deduplicate、replace、ordering、timing、filesystem、concurrencyの変更

whitespace、formatting、mechanical renameなど意味的に無関係な変更を自動的にblockしてはならない（MUST NOT）。すべてのbranch、clone、allocationにcommentを要求してはならない。対象は、straightforwardな実装から意図的に外れ、理由を失うと将来の変更判断を誤らせるcodeである。

再検証結果は次のいずれかにする。

- `Still accurate`: current implementation、failure mode、evidenceに一致する。保持する。
- `Invariant/reason changed`: protected invariantまたは理由を更新し、evidenceも更新・再確認する。
- `Reason no longer applies`: invariantが消えた、または別のevidenceで保護されていることを確認して削除する。

testの成功とrationaleの鮮度は別の証拠である。Testはbehaviorを証明し、Commentはimplementation shapeがなぜ存在するかを説明する。参照先のtestが通っていても、古い理由を残してはならない（MUST NOT）。

## Review priority

reviewの思考資源はまず製品本体へ使う。第一passでcorrectness、safety、observable semantics、UX state transition、race / failure behavior、architecture、scopeを深く確認する。

test / CI / documentation / workflowは第二passのsupporting evidenceとして確認する。supporting mechanism自体の美化や一般化へscopeを拡張せず、製品品質の証明・regression preventionを実質的に改善する問題だけを扱う。

## Review checklist

### Specification conformance

- implementationが関連する`Approved` / `Implemented` behaviorに適合しているか。
- `Draft` / `Proposed` spec、conversation、current code behaviorを未承認のimplementation authorityとして扱っていないか。
- Requirement ID referenceが実際に対象behaviorを指し、Diagnostic Codeと混同されていないか。
- Approved specで答えられないobservable behaviorを勝手に選択していないか。
- Current Objectiveのcompletion boundaryを満たし、explicit non-scopeへscope creepしていないか。

### Regression and evidence

- 変更されたbehaviorにfocused regression testがあるか、既存testが実際に保護しているか。
- test nameが`test_bug_1`のような番号だけでなく、保護するbehaviorを表しているか。
- 1つのintegration testへ複数の独立contractを詰め込み、failure localizationやCI stabilityを悪化させていないか。
- arbitrary sleep / oversized timeout / wall-clock依存ではなく、observable stateやcommand completionをdeterministicに待っているか。
- lower layerで高速・決定的に証明できるsemanticsをfull GUI/E2E testで重複検証していないか。full integration testはboundary wiringとcross-layer contractへ集中しているか。
- new/changed testがremote CIだけでtimeout / raceする場合、timeout延長の前にtest composition、wait condition、resource sensitivity、production async/raceを切り分けているか。
- fixture、golden、benchmark、external constraintが変更後の実装と対応しているか。
- `Regression:`、Requirement ID、ADR/RFC、documentation pathなどの明示参照先が存在するか。

### Rationale freshness

- non-obvious codeに必要な範囲のrecoverableなWHYがあるか。
- commentがWHATの説明ではなく、protected invariantとfailure modeを説明しているか。
- current implementationがcommentの理由と一致しているか。
- 参照testが主張されたbehaviorを実際に保護しているか。
- workaroundのaffected dependency/platform、protected behavior、removal conditionが古くなっていないか。
- refactorでrationaleが旧locationに残る、または新locationへ移動されずorphanになる状態がないか。
- optimizationにbenchmark、profile、allocation evidence、またはknown hot pathがあるか。
- unusual codeをsimplify・deleteする前に、そのinvariantを確認した証拠があるか。

### Authority and architecture

- local commentをproduct specificationの代わりにしていないか。
- local rationaleを新しいproduct requirementへ自動昇格していないか。
- Specはobservable behavior、ADRはcross-cutting architecture、Testはregression evidence、Commentはlocal implementation rationaleというowner分離を守っているか。
- core、application、CLI、GUI、codegen、.NET adapterのboundaryを越えてdomain semanticsを重複させていないか。

## 構造参照checkとの分離

`cargo xtask check-rationale`は、機械的に高い確度で検証できる次の参照だけを対象とする。

- comment内のRequirement IDがcanonical specificationで定義されていること
- `ADR-NNNN` / `RFC-NNNN`参照に対応する番号付きdocumentが存在すること
- `Regression: test_identifier`がsource内のcomment外に存在すること
- `docs/...`として明示されたrepository-relative documentation pathが存在すること

このcheckは自由記述commentの意味、WHYの正しさ、failure modeの鮮度、workaroundの必要性、benchmarkの妥当性を判定しない。独自の`@rationale`等のcomment parser DSLを導入しない。構造参照が曖昧なら自動checkを拡張せず、このskillのreview findingとして扱う。

## Finding classification

- `Blocking`: mergeするとknown correctness、spec、compatibility、architecture、またはprotected invariantに違反する。
- `Non-blocking`: approvalを妨げないeditorialまたはmaintainability concern。これだけを理由に`Ready to merge: No`としてはならない。
- `Rationale Gap`: non-obvious implementationがあるが、理由を安全に復元できない。
- `Stale Rationale`: commentがcurrent implementation、failure mode、またはcurrent evidenceと一致しない。
- `Evidence Gap`: rationaleの主張を支えるtest、spec、ADR、benchmark、またはexternal referenceが不足・破損している。
- `Specification Gap`: implementationに必要なobservable behaviorをApproved specから選べない。

`Stale Rationale`が将来のsimplificationや削除を誤らせる場合は`Blocking`でも報告する。単なる誤字や軽微なeditorial issueは`Non-blocking`とする。理由が不明なままcommentを書き換えて新しいreasonを発明してはならない。

## Severity calibrationとmerge readiness

reviewの目的は、既知のcorrectness、contract、compatibility、data safety、architecture boundary、regression、rationale/evidence riskをmerge前に止めることであり、reviewerの好みで実装を100点まで磨くことではない。

- minor naming preference、optional refactor、minor readability suggestion、editorial cleanup、future maintainability improvementは、correctness等の実質的なriskがなければ`Non-blocking`として報告する。
- correctness、spec conformance、compatibility、data safety、architecture boundary、またはprotected invariantに実質的なriskがあるものを`Non-blocking`へ格下げしてはならない。
- `Blocking`がなく、残る指摘が`Non-blocking`だけなら、必要に応じて指摘を報告したうえで`Ready to merge: Yes`としてよい。
- `Blocking`がある場合だけ、原則としてcorrective implementation passを要求する。

## Corrective pass protocol

final verificationで`Blocking` findingが残った場合、次のimplementation passは具体的なfindingを入力とするdiff-directed correctionでなければならない。

```text
Base: previous implementation commit
Input: concrete Blocking findings
Scope: those findingsの解消
Do not: Approved objective全体の再設計、unrelated refactor、new product semantics、optional cleanup
```

correction activityを実行するagentは、blocking fix、focused tests、self-review、required validation、scope確認、commit / pushまでを閉じる。修正にSpecification Gap、Approved authority conflict、Human Approval、unknown destructive semantics、またはunrecoverable rationaleが必要な場合は、semantic decisionを発明せず、明確なGap reportを返す。corrective passを新規implementation taskと同じ巨大な探索・再設計へ戻してはならない。

## Final verificationとDevelopment State

Development Stateが`verification-ready`の場合、このskillのfresh review結果とrepository-required CI evidenceを合わせて次のstageを決める。

- fresh reviewに`Blocking`なし + required remote CI success -> `objective-complete`
- fresh reviewに`Blocking`なし + required remote CI pending / infrastructure-only unresolved -> `verification-ready`を維持
- Approved authority内で修正可能なreview finding、またはproduct / test / evidence CI failure -> `correction-ready`へ遷移し、concrete findingをdurably記録
- new semantic / product / compatibility decisionまたはHuman Approvalが必要 -> `decision-required`へ遷移し、必要なdecisionだけを記録

self-reviewとしてこのskillを使っている途中は、final candidateになる前の一時findingをDevelopment Stateへ逐次記録する必要はない。Approved authority内で安全に直せるBlockingは同じimplementation work package内で修正する。

## 必須のreview report

次の構造で短く報告する。

### Scope

reviewしたdiff、implementation boundary、関連spec/test/evidence。

### Specification Conformance

`Pass` または具体的なissue。

### Tests and Regression Evidence

関連test、fixture、benchmarkの十分性と参照整合性。

### Rationale Freshness

`Fresh`、`Stale`、`Missing`、`Not applicable` を、対象ごとに理由付きで記録する。

### Evidence Integrity

- Requirement references:
- ADR/RFC references:
- Regression test references:
- Benchmark/external references:

### Architecture

boundary violationの有無。

### Findings

severity付きで`Blocking`、`Non-blocking`、`Rationale Gap`、`Stale Rationale`、`Evidence Gap`、`Specification Gap`を分類する。空なら`None identified`と書く。

### Verdict

`Ready to merge: Yes` または `Ready to merge: No`。ただしこのskillはspec statusを変更せず、semantic gapを自動修正しない。

## 通常のcompletion flow

`implement-spec`またはApproved specに対するbug fixでは、implementation中のself-reviewを次の順で実行する。

```text
implementation completed
        -> tests / regression evidence
        -> re-scan touched rationale
        -> review-code self-review
        -> Blocking fixes, if safely possible
        -> affected tests / review-code re-check
        -> cargo xtask check-rationale
        -> cargo xtask check-all
        -> diff / scope self-review
        -> commit / push
        -> Development State: verification-ready
```

tests passだけ、またはreferenceが存在するだけでreviewを省略してはならない。verification-readyへ渡すのはこのself-review後のfinal candidateであり、rationaleが不要になった場合も、protected invariantが消えたか別のevidenceへ移ったことを確認してから削除する。
