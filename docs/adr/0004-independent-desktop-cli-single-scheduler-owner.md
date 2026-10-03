# ADR 0004: 独立したDesktop/CLIディストリビューションと単一スケジューラ所有権モデル

Status: Approved

## 背景（Context）

Codex Scheduler はこれまで macOS の Gatekeeper 対策として「1 App / 1 Executable / 2 Modes」を導入し、Desktop GUI 単体での安全な予約実行を実現した（ADR 0003）。
しかし、GUI を必要とせずターミナルやリモート開発環境（SSH / エージェント自動化等）でヘッドレスに予約実行を行いたい CLI ユーザーや、両方をインストールして CLI を control plane として活用したいユーザーの要求が存在する。

一方、CLI を独立した distribution として提供する場合、以下の新たな課題とリスクが生じる：
1. **旧アーキテクチャへの逆戻りリスク**: Desktop GUI が再び外部 CLI に依存する設計に戻ってしまうと、ADR 0003 で解消した Gatekeeper 拒否問題が再発する。
2. **LaunchAgent の重複と競合**: Desktop と CLI がそれぞれ LaunchAgent を登録すると、二重実行や「バックグラウンド項目が追加されました」通知の重複が発生する。
3. **データとセマンティクスの分裂**: GUI と CLI で別々のデータストアや異なる実行・リトライロジックを持つと、状態の不整合や二重予約が発生する。

## 決定（Decision）

1. **独立した2つの正式ディストリビューション（Independent Distributions）**:
   - **Desktop distribution**: GUI アプリケーション（`Codex Scheduler.app` / Windows インストーラ）。単体で予約実行が完結し、CLI に依存しない。
   - **CLI distribution**: standalone CLI（`codex-scheduler`）。単体で予約実行が完結し、Desktop に依存しない。
2. **共有コアと共有 JobStore（Shared Core & Shared JobStore）**:
   - データストアは一元化し、常に `~/.codex-scheduler/jobs.json` を共有する。
   - ジョブモデル、バリデーション、ファイルロック、アトミッククレーム、リトライ判定、Codex アダプタ、実行履歴、tick 実行ロジックはすべて `codex-scheduler-core` で一元管理し、ロジックの複製を禁止する。
3. **macOS 単一常設 LaunchAgent 不変条件（Single Persistent LaunchAgent）**:
   - macOS 上の LaunchAgent ラベルは引き続き `dev.codexscheduler.scheduler` の1つのみとする。
   - 複数 LaunchAgent の登録は厳格に禁止する。
4. **所有権モデルと優先度ルール（Ownership Model & Precedence）**:
   - **Desktop 優先（Desktop Precedence）**:
     - 有効な Desktop 所有の登録が存在する場合、CLI はこれを尊重し上書きしない（MUST NOT overwrite）。
     - CLI 所有の状態で Desktop GUI が起動した場合、Desktop が安全に所有権を引き継ぐ（migrate）。
   - **CLI-only の自律性（CLI Autonomy）**:
     - Desktop 未インストール環境では、CLI 自身が LaunchAgent の実行主体（`codex-scheduler --scheduler-tick`）として常設登録・稼働可能とする。
   - **安全なアンインストール（Ownership-Aware Uninstall）**:
     - CLI の `uninstall-scheduler` は、現登録が Desktop 所有である場合エラーとして保護し、Desktop 所有スケジューラの破壊を防止する。
   - **消失した stale 登録の自己修復**:
     - 登録先バイナリが削除・消失している場合は、起動された利用可能な frontend が安全に上書き・修復できる。
   - **未知・破損設定の保護**:
     - 手動編集等による未知・破損設定（malformed）は無言で破壊せず、構造化エラーを報告する。
5. **CLI 公開コマンド名と機械可読性（Public Contract & Automation）**:
   - 正式公開コマンド名は `codex-scheduler` とする（Cargo `[[bin]]` によりパッケージ名と分離）。
   - エージェント自動化・リモート連携のため、全主要コマンドに `--json` オプションを提供し、成功時はクリーンな JSON のみ、失敗時は非ゼロ終了コードと構造化エラーを保証する。

## 却下された代替案（Rejected Alternatives）

1. **Desktop が standalone CLI をランタイム依存として必須とする構成**:
   - 【却下理由】ADR 0003 の決定に反し、macOS Gatekeeper による別バイナリ拒否が再発するため不可。
2. **Desktop 用と CLI 用で2つの LaunchAgent を並行稼働させる構成**:
   - 【却下理由】同一ジョブの二重実行リスクが増大し、macOS のバックグラウンド通知が重複して UX を著しく破壊するため不可。
3. **Desktop と CLI で別々の `jobs.json` を持つ構成**:
   - 【却下理由】CLI で予約したジョブが GUI 一覧に表示されず、GUI からのキャンセルも CLI に反映されないため、製品としての一体感が失われる。
4. **CLI-only 利用者にも初回のみ Desktop GUI の起動を強制する案**:
   - 【却下理由】ヘッドレス環境（SSH サーバー等）で GUI を持たない CLI ユーザーが利用不能となるため不可。

## 結果（Consequences）

- **ポジティブな影響**:
  - Desktop ユーザーは単体で完結し、Gatekeeper の心配なく直感的に利用できる。
  - CLI ユーザーは GUI なしでターミナル完結で利用でき、自動化スクリプトや AI エージェント（Claude Code / Codex 等）からの制御が容易になる。
  - 両方をインストールした場合でも、LaunchAgent は自動的に Desktop 優先の1本に統合され、二重実行や通知重複が防止される。
  - CLI から予約したジョブを Desktop GUI で監視・操作でき、シームレスな体験が実現する。
- **トレードオフと必要な設計**:
  - `codex-scheduler-core` に LaunchAgent の登録内容から所有者（Desktop / CLI / None / Legacy / Invalid）を判定するロジックが必要。
  - CLI にも `--scheduler-tick` に応答するヘッドレス tick 実行ループが必要（shared core `execute_tick` を呼び出す）。
  - リリースパイプラインにおいて、Desktop パッケージ（DMG / EXE）と standalone CLI バイナリの両方をビルド・ステージングする必要がある。
