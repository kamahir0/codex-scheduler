# 仕様ワークフロー

Workflow status: Active

この文書はDraft / Proposed specificationをApproved implementation authorityへ進めるlifecycleと、Human-selected Objective内でのAgent Decision / autonomous approvalを定義する。Human gateそのもののcriteriaは[Repository Development Workflow](../execution-workflow.md#human-gate)だけが所有し、ここへ複製しない。documentation retentionは[Documentation Policy](documentation-policy.md)に従う。

## Canonical path

```text
request / Current Objective
        -> intent + repository evidence
        -> refine-spec
        -> Draft / Proposed
        -> review-spec
        -> Human gate ? Human decision : autonomous approval when eligible
        -> Approved / Applied
        -> implement-spec
        -> tests / review / verification
        -> Implemented when evidence is complete
```

Human gateがなければ各段階の間にHuman promptを挟む必要はない。

## Evidence classification

normative textを書く前に必要なstatementを分類する。

- `Decision`: Human / durable authorityが明示確定したchoice
- `Requirement`: desired capability / outcome
- `Constraint`: boundary / prohibition / condition
- `Preference`: non-binding favored option
- `Proposal`: candidate solution
- `Idea`: exploratory possibility
- `Question`: clarification request
- `Open Question`: unresolved behavior
- `Rejected`: rejected option / behavior
- `Agent Decision`: Current Objectiveを実現するためagentが選択したObjective-local decision

Agent DecisionをHuman発言として記録してはならない（MUST NOT）。

## Agent-resolvable decision

Open Questionを機械的にHumanへ戻さない。次をすべて満たす場合、refinement passでAgent Decisionとして解決してよい（MAY）。

- [Human gate](../execution-workflow.md#human-gate)に該当しない。
- selected Current Objectiveのscope内で必要または自然。
- existing Approved authority / Product Direction / architectureと矛盾しない。
- alternativeと選択理由を説明できる。
- observable outcomeをtestできる。

implementation convenienceだけを理由にobservable behaviorを選ばない。materialなuncertaintyが残るならHuman gateまたはexplicit non-scopeへ送る。

## Normative strength

Human evidenceのstrengthを勝手に強めない。

- `MUST / MUST NOT`: explicit requirement / constraint、またはreview可能に記録されたAgent Decision
- `SHOULD / SHOULD NOT`: strong default with documented exception rationale
- `MAY`: permission / capability

Agent DecisionはHuman intentの翻訳として偽装しない。

## refine-spec

refine-specはcanonical owner、affected Requirement ID、source evidence、Agent Decision、compatibility、acceptance、Open Questionを整理し、review可能な`Draft` / `Proposed`を作る。

Approved / Implemented canonical documentへのsemantic deltaは`docs/spec-changes/` artifactへ置く（MUST）。未reviewed semanticsをcanonicalへ直接混ぜない。

refine-spec pass自身はstatusを`Approved`へ変更しない。authoringとchallenge reviewを分離するためであり、Human promptを要求するためではない。

## review-spec

review-specはproposal確定後のfresh challenge passとして、intent fidelity、cross-spec consistency、normative strength、testability、compatibility、Human gate classification、documentation ownershipを確認する。

review pass自身はstatus transitionを行わない。

結果には少なくとも次を含める。

- Blocking Issues
- Non-blocking Issues
- Questions
- Approved as Proposed
- Autonomous approval eligibility
- Human gate

## Approval lifecycle

canonical specification:

`Draft -> Proposed -> Approved -> Implemented`

specification-change artifact:

`Draft -> Proposed -> Approved -> Applied` または `Rejected`

`Approved`はimplementation authorityとして採用済みであることを意味し、approval provenanceはchange artifact / review recordへ残す。

### Autonomous approval

Proposed changeは次をすべて満たす場合agentがapprove/applyしてよい（MAY）。

1. Human-selected Current Objectiveのscope内。
2. Human gateが`None`。
3. review-specでBlockingなし。
4. materialなunresolved ambiguityなし。
5. compatibility impactが既存Approved policy内で明確。
6. success / failure / regression evidenceがtestable。
7. Human evidenceとAgent Decisionが区別されている。
8. Approval Recordへ`Approval mode: Agent-autonomous`、Objective/request basis、review resultを残す。

同じagentがrefinementとreviewを行ってもよいが、reviewはproposal確定後の別passとし、authoring中の私的意図をevidenceにしない。

### Human approval

Human gateがあるchangeはHuman decisionを取得し、`Approval mode: Human`とdecision basisをApproval Recordへ残す。approval後はcanonical applicationのためだけに追加確認を要求しない。

## Approved specificationの変更

Approved / Implemented canonical documentへsemantic changeを直接混ぜない。change artifactをreviewし、有効なapproval後にcanonicalへatomic mergeする。

Implemented documentへsemantic deltaを適用した場合、新behaviorのimplementation evidenceが揃うまでcanonical statusを`Approved`へ戻す。

Requirement IDはstableに保つ。意味をmaterially置換 / 分割する場合だけhistory / predecessorを考慮して新IDを使う。

## Ownership / normalization

1 semantic rule = 1 canonical owner。関連documentはcopyせずlinkする。

Requirement ID追加前に既存definition / wordingを検索する。runtime Diagnostic Code、test name、current implementation behaviorをrequirement authorityとして流用しない。Accepted RFCはimplementation authorityではない。

Product terminologyはglossary/routingとして使い、term自体がobservable contractを所有しないようcanonical specへrouteする。

## Compatibility

stable identity、serialized shape、generated API、file interpretation、protocol、config、CLI contract等へのimpactを明記する。Human gate classificationはexecution workflowへ委ねる。

## Tests / traceability

Approved behaviorには規模に応じたfocused verification evidenceを用意する。traceabilityに有用ならRequirement IDをtest nameまたは近接commentへ置く。stable end-to-end input自体がcontract理解に有用ならfixture、小さなruleはunit/integration testで十分。

forward traceability `Spec -> Test -> Implementation` と、必要なlocal WHYのreverse traceabilityを維持する。どこへ何を残すかはDocumentation Policy、local rationaleの書き方はImplementation Rationale guideに従う。

## Applied artifact retention

`Applied` artifactはcurrent implementation authorityではない。[Documentation Policy](documentation-policy.md#specification-change-retention)に従い、current checkoutではWhy、canonical owner / Requirement ID、approval/application traceabilityだけのcompact audit recordへ縮退してよい。詳細proposal/reviewはGit historyが保持する。

## Current Objective

Current ObjectiveはWHAT / DONE boundaryだけを所有する。Requirement本文、failure semantics、test inventory、細かい進捗をcopyしない。resume地点はDevelopment State、implementation realityはGit/code/testsが所有する。

## Workflow hygiene

workflowの目的はHuman synchronization回数やdocumentation量を最大化することではない。incidentで繰り返すfailure modeが判明した場合だけ、最小のhard invariant、focused test、owner ruleを追加する。
