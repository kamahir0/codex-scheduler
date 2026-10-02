---
name: release
description: Automate release workflow using cargo xtask release, including SemVer determination, verification, commit, tag, and push.
---

# release

## 目的

`codex-scheduler` の新しいバージョンを安全・確実にリリースし、GitHub Actions によるマルチプラットフォーム自動ビルド（macOS DMG / Windows インストーラ）を起動する。

## 発動トリガー

- ユーザーから「リリース」「リリースして」「vX.Y.Z をリリース」等の指示があったとき。
- Current Objective や仕様変更タスクが完了し、リリース準備が整ったとき。

## SemVer（バージョン）判断基準

1. **パッチリリース (`patch`, 例: 0.2.1 -> 0.2.2)**:
   - バグ修正、軽微なUI調整、スタイルの微修正、ドキュメント整備など後方互換性のある修正。
2. **マイナーリリース (`minor`, 例: 0.2.1 -> 0.3.0)**:
   - 新機能追加、新規画面・モーダル追加、設定項目の追加、アーキテクチャ改修など後方互換性のある機能追加。
3. **メジャーリリース (`major`, 例: 0.2.1 -> 1.0.0)**:
   - 既存APIやジョブスキーマの破壊的変更、大幅なプロダクト再設計など。
4. **明示指定 (`X.Y.Z`)**:
   - ユーザーから明示的なバージョン番号が指定された場合は、そのバージョンを採用する。

## 実行プロシージャ

### 1. 作業ツリーの確認

ワーキングツリーが clean であることを確認する：
```bash
git status --porcelain
```
未コミットの変更がある場合は、リリース前に適切なコミットを作成する。

### 2. 直近の変更サマリーの確認

前回リリースタグからの差分を確認し、リリースノートの要約を作成する：
```bash
git log $(git describe --tags --abbrev=0)..HEAD --oneline
```

### 3. リリースタスクの実行

`cargo xtask release` コマンドを実行する：
```bash
# パッチリリースの例
cargo xtask release patch -m "変更サマリーの要約"

# マイナーリリースの例
cargo xtask release minor -m "変更サマリーの要約"

# 明示的バージョン指定の例
cargo xtask release 0.3.0 -m "変更サマリーの要約"
```

※ npm script 経由でも同一のタスクが実行可能：
```bash
npm run release:patch
npm run release:minor
```

### 4. 実行内容（自動化される処理）

`cargo xtask release` により以下が一貫して実行される：
1. `Cargo.toml` (`[workspace.package] version`) の更新
2. `package.json`（ルート）の `version` 更新
3. `apps/gui/package.json` の `version` 更新
4. `apps/gui/src-tauri/tauri.conf.json` の `version` 更新
5. `cargo check` による `Cargo.lock` の同期
6. `cargo test --workspace --exclude xtask` および `npm run frontend:build` によるビルド・テスト検証
7. バージョン更新コミットの作成
8. `docs/execution-state.md` の Candidate ハッシュ更新＆コミット
9. アノテーション付き Git タグ（`vX.Y.Z`）の作成
10. リモート `origin/main` および `vX.Y.Z` タグへのプッシュ

### 5. リリースパイプラインの監視・報告

プッシュ後、GitHub Actions ワークフローが起動したことを確認し、ユーザーへバージョン番号と成果物の配布状況を報告する：
```bash
gh run list --limit 2
```
