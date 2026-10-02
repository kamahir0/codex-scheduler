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
        <Descriptions.Item label="常設 Worker CLI 状態">
          {systemInfo.cli_worker_installed ? (
            <Tag color="green">常駐配置済 (Ready)</Tag>
          ) : (
            <Tag color="red">未配置 (Not Installed)</Tag>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="常設 Worker パス">
          <Text copyable style={{ fontSize: 12, fontFamily: "monospace" }}>
            {systemInfo.cli_worker_path}
          </Text>
        </Descriptions.Item>
        <Descriptions.Item label="ジョブストア保存先">
          <Text copyable style={{ fontSize: 12, fontFamily: "monospace" }}>
            {systemInfo.jobs_store_path}
          </Text>
        </Descriptions.Item>
        <Descriptions.Item label="OSスケジューラ権限">
          <Tag color="cyan">
            {systemInfo.os === "macos"
              ? "macOS LaunchAgent (管理者権限不要)"
              : "Windows Task Scheduler"}
          </Tag>
        </Descriptions.Item>
      </Descriptions>

      <div style={{ marginTop: 20, padding: 12, background: "rgba(0,0,0,0.02)", borderRadius: 6 }}>
        <Paragraph style={{ margin: 0, fontSize: 12, color: "#8c8c8c" }}>
          ※ <strong>完全無料配布・権限維持アーキテクチャ</strong>:
          本アプリは Apple Developer Program 有償登録なしで利用できるよう、GUI 本体（更新・置換対象）と常設 Worker CLI（固定パス永続化）を権限分離しています。
          GUI をアップデート置換しても既存の登録ジョブや macOS TCC 権限は維持されます。
          初回起動時の Gatekeeper 警告等については、<strong>セットアップガイド（docs/setup-guide.md）</strong> を参照してください。
        </Paragraph>
      </div>
    </Modal>
  );
};
