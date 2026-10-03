import React from "react";
import { Modal, Descriptions, Tag, Typography, Alert, Button, Space } from "antd";
import { CheckCircle2, AlertTriangle, ShieldCheck, Terminal, ExternalLink } from "lucide-react";
import { SystemInfo } from "../types";

const { Text, Paragraph } = Typography;

interface DiagnosticsModalProps {
  open: boolean;
  onClose: () => void;
  systemInfo: SystemInfo | null;
}

export const DiagnosticsModal: React.FC<DiagnosticsModalProps> = ({
  open,
  onClose,
  systemInfo,
}) => {
  if (!systemInfo) return null;

  return (
    <Modal
      title={
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <ShieldCheck size={20} color="#1677ff" />
          <span>システム環境 & 権限診断</span>
        </div>
      }
      open={open}
      onCancel={onClose}
      footer={
        <Button type="primary" onClick={onClose}>
          閉じる
        </Button>
      }
      width={640}
    >
      <div style={{ marginBottom: 16 }}>
        {systemInfo.scheduler_error && (
          <Alert
            message="OSスケジューラ登録エラー"
            description={systemInfo.scheduler_error}
            type="error"
            showIcon
            style={{ marginBottom: 16 }}
          />
        )}
        {systemInfo.codex_installed ? (
          <Alert
            message="Codex CLI が正常に検出されました"
            description={`実行パス: ${systemInfo.codex_path || "PATH 内で検出"}`}
            type="success"
            showIcon
            icon={<CheckCircle2 size={18} color="#52c41a" />}
            style={{ marginBottom: 16 }}
          />
        ) : (
          <Alert
            message="Codex CLI が検出されませんでした"
            description="深夜の自動再開を行うには、システムに `codex` コマンドがインストールされ、PATHが通っている必要があります。"
            type="warning"
            showIcon
            icon={<AlertTriangle size={18} color="#faad14" />}
            style={{ marginBottom: 16 }}
          />
        )}
      </div>

      <Descriptions title="環境ステータス" bordered size="small" column={1}>
        <Descriptions.Item label="OS / プラットフォーム">
          <Tag color="geekblue">{systemInfo.os.toUpperCase()}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="Codex CLI 状態">
          {systemInfo.codex_installed ? (
            <Tag color="green">利用可能 (Ready)</Tag>
          ) : (
            <Tag color="orange">未検出 (Not Found)</Tag>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="Codex 検出パス">
          <Text copyable style={{ fontSize: 12, fontFamily: "monospace" }}>
            {systemInfo.codex_path || "None"}
          </Text>
        </Descriptions.Item>
        <Descriptions.Item label="スケジューラ実行方式">
          <Tag color="purple">
            {systemInfo.os === "macos"
              ? "シングル実行ファイル・ヘッドレスモード (Gatekeeper 対策済)"
              : "OS スケジューラ連携"}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="スケジューラ実行パス">
          <Text copyable style={{ fontSize: 12, fontFamily: "monospace" }}>
            {systemInfo.scheduler_executable_path}
          </Text>
        </Descriptions.Item>
        <Descriptions.Item label="OSスケジューラ常設サービス">
          {systemInfo.scheduler_installed ? (
            <Tag color="green">
              {systemInfo.os === "macos"
                ? "登録済 (単一LaunchAgent: dev.codexscheduler.scheduler)"
                : "登録済 (Task Scheduler)"}
            </Tag>
          ) : (
            <Tag color="orange">未登録 (起動時またはジョブ作成時に自動登録)</Tag>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="スケジューラ稼働準備状態">
          {systemInfo.scheduler_ready ? (
            <Tag color="green">準備完了 (Ready / Loaded)</Tag>
          ) : (
            <Tag color="orange">未完了 (Not Ready)</Tag>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="実行パス一致状態">
          {systemInfo.scheduler_path_matched ? (
            <Tag color="green">一致 (Matched)</Tag>
          ) : (
            <Tag color="orange">不一致または再登録待機中 (Update Needed)</Tag>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="ジョブストア保存先">
          <Text copyable style={{ fontSize: 12, fontFamily: "monospace" }}>
            {systemInfo.jobs_store_path}
          </Text>
        </Descriptions.Item>
        <Descriptions.Item label="OSスケジューラ権限">
          <Tag color="cyan">
            {systemInfo.os === "macos"
              ? "macOS LaunchAgent (管理者権限不要・通知初回のみ)"
              : "Windows Task Scheduler"}
          </Tag>
        </Descriptions.Item>
      </Descriptions>

      <div style={{ marginTop: 20, padding: 12, background: "rgba(0,0,0,0.02)", borderRadius: 6 }}>
        <Paragraph style={{ margin: 0, fontSize: 12, color: "#8c8c8c" }}>
          ※ <strong>バックグラウンド通知抑制 & シングル実行ファイルアーキテクチャ</strong>:
          LaunchAgent は別バイナリではなく、現在起動中のアプリ本体をヘッドレスモード（<code>--scheduler-tick</code>）で起動します。これにより、初回起動時にアプリを1回許可するだけでバックグラウンド実行に対する追加の Gatekeeper 警告を防ぎ、ジョブ追加ごとの通知も発生しません。
        </Paragraph>
      </div>
    </Modal>
  );
};
