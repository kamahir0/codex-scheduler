# ADR 0003: macOS シングル実行ファイル・ヘッドレススケジューラアーキテクチャ

Status: Approved

## 背景（Context）

本プロジェクトは、Apple Developer Program の有償アカウントによる公証（Notarization）を使用せず、ad-hoc コード署名（`signingIdentity: "-"`）を用いた完全無料のオープンソース配布を前提としている。

以前のアーキテクチャ（ADR 0001, Spec Change 0008, 0012）では、更新される GUI アプリ本体（`.app`）と OS スケジューラ実行主体を分離するため、GUI 内に同梱された `codex-scheduler-cli` を初回起動時に外部固定パス（`~/.local/share/codex-scheduler/bin/codex-scheduler-cli`）へコピーし、macOS LaunchAgent からこの別バイナリを定期実行していた。

しかし実機検証において、以下の重大な問題が発生した：
1. **Gatekeeper による別バイナリ拒否**: GUI アプリ（`Codex Scheduler.app`）はユーザーが明示的に初回起動を許可（Control+クリックで「開く」など）して動作可能となっても、外部に配置された別 Mach-O 実行ファイルである `codex-scheduler-cli` は別個の Gatekeeper 対象となり、「“codex-scheduler-cli”は開いていません」という警告が出てバックグラウンド実行がブロックされる。
2. **パス安定性とコード信頼（Identity）の不一致**: 固定パスにバイナリを配置しても、アプリ更新時にバイナリが上書きされればハッシュ値が変化するため、ad-hoc 署名環境において Gatekeeper や TCC の信頼状態が永続する保証はない。
3. **実行信頼性の乖離**: GUI での手動実行（「今すぐ実行」）は動作するが、launchd からの予約実行が失敗するというユーザー体験上の断絶が生じる。

## 決定（Decision）

1. **シングルバイナリ・2モード実行（1 App / 1 Executable / 2 Modes）の採用**:
   - macOS デスクトップ版において、外部の `codex-scheduler-cli` バイナリを OS スケジューラの実行主体とする方式を完全に廃止する。
   - `Codex Scheduler.app` 内のメイン GUI 実行ファイル（`Contents/MacOS/codex-scheduler-gui`）自体にヘッドレス実行モード（`--scheduler-tick`）を実装し、1つのバイナリで通常 GUI とバックグラウンドスケジューラの両方を担う。
2. **LaunchAgent の実行対象をメインアプリへ変更**:
   - 単一常設 LaunchAgent（`dev.codexscheduler.scheduler.plist`）の `ProgramArguments` には、`std::env::current_exe()` から動的に取得した現在インストールされているアプリ本体の絶対パスと `--scheduler-tick` を登録する。
3. **ヘッドレスモードの分離**:
   - `--scheduler-tick` で起動された場合は、Tauri ウィンドウ、Dock アイコン、WebView、GUI ダイアログ等のグラフィカルリソースを一切初期化せず、バックグラウンドプロセスとして即座に待機中ジョブ（`due jobs`）を実行して終了する。
4. **共有コアセマンティクス（Shared Core Semantics）の維持**:
   - ジョブ抽出、重複実行防止（atomic claim）、実行、リトライ、履歴記録はすべて `codex-scheduler-core` の共通ロジックを使用し、GUI 手動実行と同一のセマンティクスを共有する。
5. **別 Worker CLI のランタイム依存排除**:
   - `codex-scheduler-cli` は開発者向け・手動操作用ツールとしてリポジトリ内に残してよいが、デスクトップアプリの予約実行ランタイム依存からは完全に除外する。

## 却下された代替案（Rejected Alternatives）

1. **アプリから `xattr -d com.apple.quarantine` などの Gatekeeper 回避コマンドを自動実行する**:
   - 【却下理由】ユーザーの OS セキュリティ保護機構をアプリケーションが無断でバイパスすることは安全規約違反であり、マルウェア的挙動として厳格に禁止される。
2. **外部固定パス（`~/.local/share/...`）への Worker CLI コピーを維持する**:
   - 【却下理由】Gatekeeper の別バイナリ拒否という根本原因を解決できず、実機での予約実行失敗が解消されない。
3. **`.app` バンドル内のリソースバイナリ（`Contents/Resources/codex-scheduler-cli`）を LaunchAgent から直接起動する**:
   - 【却下理由】バンドル内であっても別 executable であることには変わりなく、macOS が別プロセスとして Gatekeeper 評価を行いブロックする根本リスクが残る。
4. **ジョブごとの LaunchAgent（`com.codexscheduler.job.<job_id>`）へ戻す**:
   - 【却下理由】ジョブを1件追加するたびに macOS から「バックグラウンド項目が追加されました」と通知される過去の UX 破壊（通知スパム）が再発するため不可。

## 結果（Consequences）

- **ポジティブな影響**:
  - ユーザーが `Codex Scheduler.app` を一度許可して起動すれば、バックグラウンドの launchd も同じ許可済みバイナリを呼ぶため、追加の Gatekeeper 拒否が一切発生しなくなる。
  - 外部パスへのバイナリコピー・プロビジョニングやパーミッション管理（0o755設定など）の複雑性が完全に解消される。
  - GUI アプリとバックグラウンド実行でバイナリ不一致やバージョンズレが起きなくなる。
- **トレードオフと必要な設計**:
  - メイン GUI 実行ファイルのエントリポイントで引数を判定し、Tauri 初期化前にヘッドレス実行へ分岐させる処理が必要。
  - ユーザーがアプリを移動したり更新した場合に LaunchAgent の登録パスが追従できるよう、起動時にパスの差分比較と安全な再登録（内容同一時は再登録をスキップし通知を抑止）が必要。
  - LaunchAgent が60秒間隔で起動された際、前の実行が長引いた場合の二重実行を防ぐ atomic claim 機構がコアに必要。
