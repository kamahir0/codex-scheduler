import React, { useState, useEffect } from "react";
import {
  Modal,
  Form,
  Input,
  DatePicker,
  Select,
  InputNumber,
  Switch,
  Space,
  Button,
  Tag,
  Alert,
  message,
} from "antd";
import { FolderOpen, Sparkles, HelpCircle } from "lucide-react";
import dayjs, { Dayjs } from "dayjs";
import { CreateJobPayload, SystemInfo } from "../types";
import { pickDirectory } from "../api";

interface CreateJobModalProps {
  open: boolean;
  onCancel: () => void;
  onSubmit: (payload: CreateJobPayload) => Promise<void>;
  systemInfo: SystemInfo | null;
}

export const CreateJobModal: React.FC<CreateJobModalProps> = ({
  open,
  onCancel,
  onSubmit,
  systemInfo,
}) => {
  const [form] = Form.useForm();
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (open) {
      // Default to next 02:05 AM if in the evening, or 1 hour later
      const now = dayjs();
      let defaultTarget = now.hour(2).minute(5).second(0);
      if (defaultTarget.isBefore(now)) {
        defaultTarget = defaultTarget.add(1, "day");
      }

      form.setFieldsValue({
        provider: "codex",
        session_id: "",
        cwd: systemInfo?.default_cwd || "",
        prompt: "continue",
        scheduled_at: defaultTarget,
        retry_enabled: true,
        retry_interval_seconds: 300,
        max_attempts: 6,
      });
    }
  }, [open, systemInfo, form]);

  const handleBrowseFolder = async () => {
    const selected = await pickDirectory();
    if (selected) {
      form.setFieldValue("cwd", selected);
    }
  };

  const handleSetPresetTime = (preset: "2am" | "205am" | "1hour" | "30min") => {
    const now = dayjs();
    let target: Dayjs;
    if (preset === "2am") {
      target = now.hour(2).minute(0).second(0);
      if (target.isBefore(now)) target = target.add(1, "day");
    } else if (preset === "205am") {
      target = now.hour(2).minute(5).second(0);
      if (target.isBefore(now)) target = target.add(1, "day");
    } else if (preset === "1hour") {
      target = now.add(1, "hour");
    } else {
      target = now.add(30, "minute");
    }
    form.setFieldValue("scheduled_at", target);
  };

  const handleFinish = async (values: any) => {
    setLoading(true);
    try {
      const scheduledAt: Dayjs = values.scheduled_at;
      const payload: CreateJobPayload = {
        provider: values.provider,
        session_id: values.session_id.trim(),
        cwd: values.cwd.trim(),
        prompt: values.prompt?.trim() || "continue",
        scheduled_at: scheduledAt.toISOString(),
        retry_enabled: values.retry_enabled,
        retry_interval_seconds: values.retry_interval_seconds || 300,
        max_attempts: values.max_attempts || 6,
      };

      await onSubmit(payload);
      message.success("ジョブをスケジュールしました");
      form.resetFields();
      onCancel();
    } catch (e: any) {
      message.error(e.message || "スケジュールの登録に失敗しました");
    } finally {
      setLoading(false);
    }
  };

  return (
    <Modal
      title={
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Sparkles size={20} color="#1677ff" />
          <span>新しい再開ジョブをスケジュール</span>
        </div>
      }
      open={open}
      onCancel={onCancel}
      footer={null}
      width={620}
      destroyOnClose
    >
      <Alert
        message="夜間トークン復活時の自動再開"
        description="Codexデスクトップアプリで停止したセッションのIDと作業ディレクトリを入力してください。指定時刻にOSバックグラウンドで自動的に 'continue' を送信し、翌朝デスクトップアプリから同一セッションの続きを再開できます。"
        type="info"
        showIcon
        style={{ marginBottom: 20 }}
      />

      <Form form={form} layout="vertical" onFinish={handleFinish}>
        <Form.Item label="AI プロバイダ" name="provider" initialValue="codex">
          <Select
            options={[
              { value: "codex", label: "OpenAI Codex (Desktop & CLI共有)" },
              { value: "claude", label: "Anthropic Claude Code" },
            ]}
          />
        </Form.Item>

        <Form.Item
          label="セッション ID (Session ID)"
          name="session_id"
          rules={[{ required: true, message: "Session IDを入力してください" }]}
          extra="Codexセッションの識別子（例: 019abc...）。CodexアプリのURLや履歴から取得できます。"
        >
          <Input placeholder="例: 019abcde-1234-789a-bcde-f0123456789a" />
        </Form.Item>

        <Form.Item
          label="作業ディレクトリ (Project Working Directory)"
          name="cwd"
          rules={[{ required: true, message: "作業ディレクトリを指定してください" }]}
          extra="Codexがプロジェクトのコンテキストを正しく参照するためのフォルダパスです。"
        >
          <Space style={{ width: "100%" }}>
            <Input
              style={{ width: "420px" }}
              placeholder="/Users/name/develop/my-project"
            />
            <Button icon={<FolderOpen size={16} />} onClick={handleBrowseFolder}>
              参照
            </Button>
          </Space>
        </Form.Item>

        <Form.Item
          label="送信プロンプト"
          name="prompt"
          initialValue="continue"
          extra="利用枠復活時に自動送信する指示文。通常は 'continue' のままで問題ありません。"
        >
          <Input placeholder="continue" />
        </Form.Item>

        <Form.Item
          label="実行予定日時"
          name="scheduled_at"
          rules={[{ required: true, message: "実行日時を選択してください" }]}
        >
          <DatePicker
            showTime={{ format: "HH:mm" }}
            format="YYYY-MM-DD HH:mm"
            style={{ width: "100%", marginBottom: 8 }}
          />
        </Form.Item>

        <div style={{ marginBottom: 20 }}>
          <Space size={6} wrap>
            <span style={{ fontSize: "12px", color: "#8c8c8c" }}>クイック指定:</span>
            <Tag
              color="blue"
              style={{ cursor: "pointer" }}
              onClick={() => handleSetPresetTime("205am")}
            >
              今夜 02:05 (推奨)
            </Tag>
            <Tag
              color="cyan"
              style={{ cursor: "pointer" }}
              onClick={() => handleSetPresetTime("2am")}
            >
              今夜 02:00
            </Tag>
            <Tag
              style={{ cursor: "pointer" }}
              onClick={() => handleSetPresetTime("1hour")}
            >
              1時間後
            </Tag>
            <Tag
              style={{ cursor: "pointer" }}
              onClick={() => handleSetPresetTime("30min")}
            >
              30分後
            </Tag>
          </Space>
        </div>

        <div style={{ background: "rgba(0,0,0,0.02)", padding: 14, borderRadius: 8, marginBottom: 24, border: "1px solid rgba(0,0,0,0.06)" }}>
          <div style={{ fontWeight: 600, marginBottom: 12, display: "flex", alignItems: "center", gap: 6 }}>
            <span>利用枠リセット待ちリトライ設定</span>
          </div>

          <Form.Item
            name="retry_enabled"
            valuePropName="checked"
            label="利用枠（Quota）枯渇時に自動リトライ"
            style={{ marginBottom: 12 }}
          >
            <Switch />
          </Form.Item>

          <Space size={24}>
            <Form.Item
              name="retry_interval_seconds"
              label="リトライ間隔 (秒)"
              initialValue={300}
              style={{ marginBottom: 0 }}
            >
              <InputNumber min={30} max={3600} step={30} addonAfter="秒 (5分)" />
            </Form.Item>

            <Form.Item
              name="max_attempts"
              label="最大試行回数"
              initialValue={6}
              style={{ marginBottom: 0 }}
            >
              <InputNumber min={1} max={30} addonAfter="回" />
            </Form.Item>
          </Space>
        </div>

        <Form.Item style={{ marginBottom: 0, textAlign: "right" }}>
          <Space>
            <Button onClick={onCancel}>キャンセル</Button>
            <Button type="primary" htmlType="submit" loading={loading}>
              ジョブをスケジュール登録
            </Button>
          </Space>
        </Form.Item>
      </Form>
    </Modal>
  );
};
