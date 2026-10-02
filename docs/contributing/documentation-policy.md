# Documentation Policy

Workflow status: Active

この文書はrepository-wideなdocumentation quantity、retention、canonical owner routingのownerである。目的はdocumentation completenessではなく、**freshなdeveloper / AIが合理的な時間で正しいdecision、現在地、WHYを復元できること**である。

## Documentation budget gate

durable informationを新しく書く前に、次を順に判定する。

1. 既存のcanonical owner、code、test、Git historyから十分に復元できるか。Yesなら追加しない。
2. regression behaviorをtest name / assertionで十分に保護できるか。Yesならtestをownerにする。
3. non-obviousなlocal implementation WHYか。Yesならnearby rationaleだけに置く。
4. observable product/domain contractか。Yesならspecificationへ置く。
5. cross-cutting architecture choice / trade-offか。YesならADRへ置く。
6. substantialな未決定alternative比較か。YesならRFCへ置く。
7. 現在のpriority / resume地点だけか。Current Objective / Development Stateへ最小限だけ置く。

「念のため」「将来使うかもしれない」「説明が多いほど安全」という理由だけでartifactやcommentを増やしてはならない。

## One knowledge, one owner

同じknowledgeを複数ownerへ文章で複製しない。

- Specification: observable contract
- ADR: cross-cutting architecture WHY
- Test: executable regression evidence
- Nearby rationale: local non-obvious WHY / failure mode / removal condition
- Current Objective: WHAT / DONE boundary
- Development State: WHERE / RESUME pointer
- Git / code: implementation reality
- RFC: substantial alternative discussion

owner外documentはlink、Requirement ID、短いrouting文だけを使う。summaryが必要でもcanonical semanticsを言い換えて第二のcontractを作らない。

## Current ObjectiveとDevelopment State

Current ObjectiveにはObjective、major completion slice、canonical Requirement references、explicit non-scopeだけを置く。failure semanticsやvalidation rule等のspec本文をcopyしない。

Development Stateはlong-running autonomous workのresume checkpointとして、Stage、Candidate、Work base、Active work、Blockingだけを持つ。Active workはwork package / Requirement IDレベルのCompleted / In progress / Remainingに限定し、chronological log、test transcript、implementation explanation、spec本文を保存しない。

## Specification change retention

Draft / Proposed中はreviewに必要なevidence、delta、compatibility、review findingをartifactへ保持する。

`Applied`になったartifactはcurrent implementation authorityではない。current checkoutでは次だけを残すcompact audit recordへ縮退してよい。

- changeのWhy / adopted decisionの短い要約
- canonical owner / Requirement ID
- Approval provenance
- canonical application commit等のtraceability

詳細なProposed wording、review transcript、implementation impactはGit historyから復元する。Applied artifactへcanonical requirement本文をcopyし続けない。

## Historical evidence

過去candidateのreview、performance measurement、manual verification等は削除する必要はないが、current authorityと明確に分離する。

`docs/evidence/**` は原則Historical Evidenceであり、Current Objective、Development State、canonical spec、active Candidateから明示参照された場合だけcold-start contextへ読む。historical findingをcurrent defectとして無検証で再利用しない。

## Document splitting

文字数だけを理由にcanonical documentを分割しない。次の状態になったとき、変更taskの一部としてtopic owner分割を検討する。

- 1つの変更に対しfileの大半が無関係になる。
- independent lifecycle / responsibilityを持つrequirement群が混在する。
- owner境界が曖昧になり、agentが毎回広い文書を読む必要がある。

分割だけを目的とした大規模rewriteをdefaultにしない。

## Anti-patterns

- specにあるruleをCurrent Objectiveへ再記述する。
- ADRのtrade-offをcode commentへ丸ごとcopyする。
- testで十分なregressionに別のevidence reportを恒久追加する。
- Applied proposalをcurrent contractとして読み続ける。
- conversation transcriptや作業日誌をrepositoryへ保存する。
- checkpointのためだけにbroken commitを作る。
- stale documentを「情報が多いから」という理由で維持する。

## Review question

documentationを追加・維持する際は次の2問で十分である。

1. この情報がないとfuture maintainerが合理的だが意図に反する変更をし得るか。
2. その情報は既存ownerから復元できず、ここが最小かつ唯一のownerか。

どちらかがNoなら、原則として新しいdurable proseを追加しない。
