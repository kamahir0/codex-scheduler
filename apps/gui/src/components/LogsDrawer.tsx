import React from "react";
import {
  Drawer,
  Descriptions,
  Timeline,
  Tag,
  Typography,
  Card,
  Space,
  Button,
  Empty,
  message,
  Spin,
  Checkbox,
} from "antd";
import { Terminal, Copy, CheckCircle, XCircle, AlertTriangle } from "lucide-react";
import { Job, ExecutionAttempt, ActiveLogResponse } from "../types";
import { getActiveJobLog } from "../api";
import dayjs from "dayjs";

const { Text, Paragraph } = Typography;

interface LogsDrawerProps {
  job: Job | null;
  open: boolean;
  onClose: () => void;
}

export const LogsDrawer: React.FC<LogsDrawerProps> = ({ job, open, onClose }) => {
  const [activeLog, setActiveLog] = React.useState<ActiveLogResponse | null>(null);
  const [loadingActive, setLoadingActive] = React.useState<boolean>(false);
  const [lastUpdated, setLastUpdated] = React.useState<string | null>(null);
  const [autoScroll, setAutoScroll] = React.useState<boolean>(true);
  const liveLogRef = React.useRef<HTMLPreElement>(null);

  React.useEffect(() => {
    if (!open || !job || job.status !== "running") {
      setActiveLog(null);
      setLastUpdated(null);
      return;
    }

    let isMounted = true;
    const fetchLog = async () => {
      try {
        setLoadingActive(true);
        const data = await getActiveJobLog(job.id);
        if (isMounted) {
          setActiveLog(data);
          setLastUpdated(dayjs().format("HH:mm:ss"));
        }
      } catch (err) {
        console.warn("Failed to fetch active job log:", err);
      } finally {
        if (isMounted) {
          setLoadingActive(false);
        }
      }
    };

    fetchLog();
    const timer = setInterval(fetchLog, 2000);
    return () => {
      isMounted = false;
      clearInterval(timer);
    };
  }, [open, job?.id, job?.status]);

  React.useEffect(() => {
    if (autoScroll && liveLogRef.current) {
      liveLogRef.current.scrollTop = liveLogRef.current.scrollHeight;
    }
  }, [activeLog?.output, autoScroll]);

  if (!job) return null;

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    message.success("ログをクリップボードにコピーしました");
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case "succeeded":
        return "success";
      case "failed":
        return "error";
      case "running":
        return "processing";
      case "retrying":
        return "warning";
      case "cancelled":
        return "default";
      default:
        return "blue";
    }
  };

  return (
    <Drawer
      title={
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Terminal size={18} color="#1677ff" />
          <span>ジョブ実行ログ & 詳細</span>
          <Tag color={getStatusColor(job.status)} style={{ marginLeft: "auto" }}>
            {job.status.toUpperCase()}
          </Tag>
        </div>
      }
      placement="right"
      width={680}
      onClose={onClose}
      open={open}
    >
      <Descriptions title="基本情報" bordered size="small" column={1} style={{ marginBottom: 24 }}>
        <Descriptions.Item label="ジョブ ID">
          <Text copyable style={{ fontSize: 12 }}>{job.id}</Text>
        </Descriptions.Item>
        <Descriptions.Item label="セッション ID">
          <Text copyable style={{ fontSize: 12 }}>{job.session_id}</Text>
        </Descriptions.Item>
        <Descriptions.Item label="作業ディレクトリ">
          <Text ellipsis style={{ maxWidth: 450, fontSize: 12 }}>{job.cwd}</Text>
        </Descriptions.Item>
        <Descriptions.Item label="プロンプト">
          <Tag color="geekblue">{job.prompt}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="実行予定日時">
          {dayjs(job.scheduled_at).format("YYYY-MM-DD HH:mm:ss")}
        </Descriptions.Item>
        <Descriptions.Item label="リトライ設定">
          {job.retry_policy.enabled
            ? `有効 (${job.retry_policy.interval_seconds}秒間隔 / 最大${job.retry_policy.max_attempts}回)`
            : "無効"}
        </Descriptions.Item>
      </Descriptions>

      {job.status === "running" && (
        <Card
          size="small"
          style={{
            marginBottom: 24,
            borderColor: "#1677ff",
            borderRadius: 8,
            boxShadow: "0 2px 8px rgba(22, 119, 255, 0.12)",
          }}
          title={
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                flexWrap: "wrap",
                gap: 8,
              }}
            >
              <Space>
                <Spin size="small" />
                <span style={{ fontWeight: 600, color: "#1677ff" }}>
                  実行中リアルタイムログ (試行 #{activeLog?.attempt_number ?? job.execution_history.length + 1})
                </span>
              </Space>
              <Space size="middle">
                {lastUpdated && (
                  <Text type="secondary" style={{ fontSize: 11 }}>
                    更新: {lastUpdated}
                  </Text>
                )}
                <Checkbox
                  checked={autoScroll}
                  onChange={(e) => setAutoScroll(e.target.checked)}
                  style={{ fontSize: 12 }}
                >
                  自動スクロール
                </Checkbox>
                {activeLog?.output && (
                  <Button
                    type="text"
                    size="small"
                    icon={<Copy size={13} />}
                    onClick={() => handleCopy(activeLog.output)}
                  >
                    コピー
                  </Button>
                )}
              </Space>
            </div>
          }
        >
          {activeLog?.log_path && (
            <div style={{ marginBottom: 6 }}>
              <Text type="secondary" style={{ fontSize: 11 }}>
                ログ保存先: <Text code style={{ fontSize: 11 }}>{activeLog.log_path}</Text>
              </Text>
            </div>
          )}
          <pre
            ref={liveLogRef}
            style={{
              background: "#0d1117",
              color: "#58a6ff",
              padding: 12,
              borderRadius: 6,
              fontSize: 12,
              lineHeight: "1.5",
              maxHeight: 280,
              overflow: "auto",
              margin: 0,
              whiteSpace: "pre-wrap",
              wordBreak: "break-all",
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
            }}
          >
            {activeLog?.output ? (
              activeLog.output
            ) : (
              <span style={{ color: "#8b949e", fontStyle: "italic" }}>
                ログ出力の到着を待機中... (Codex プロセスが起動・準備中です)
              </span>
            )}
          </pre>
        </Card>
      )}

      <div style={{ fontWeight: 600, fontSize: 16, marginBottom: 16 }}>
        実行履歴 ({job.execution_history.length} 試行)
      </div>

      {job.execution_history.length === 0 ? (
        <Empty description="まだ完了した実行履歴はありません（実行中の内容は上部に表示されます）" />
      ) : (
        <Timeline
          items={job.execution_history.map((att: ExecutionAttempt, index: number) => {
            const isSuccess = att.exit_code === 0;
            const icon = isSuccess ? (
              <CheckCircle size={16} color="#52c41a" />
            ) : att.is_quota_error ? (
              <AlertTriangle size={16} color="#fa8c16" />
            ) : (
              <XCircle size={16} color="#ff4d4f" />
            );

            const allLogs = [att.stdout, att.stderr].filter(Boolean).join("\n");

            return {
              dot: icon,
              children: (
                <Card
                  key={index}
                  size="small"
                  title={
                    <Space style={{ width: "100%", justifyContent: "space-between" }}>
                      <span>
                        試行 #{att.attempt_number} (
                        {dayjs(att.started_at).format("HH:mm:ss")})
                      </span>
                      <Space>
                        {att.is_quota_error && <Tag color="warning">利用枠（Quota）超過</Tag>}
                        <Tag color={isSuccess ? "green" : "red"}>
                          Exit Code: {att.exit_code ?? "N/A"}
                        </Tag>
                        {allLogs && (
                          <Button
                            type="text"
                            size="small"
                            icon={<Copy size={14} />}
                            onClick={() => handleCopy(allLogs)}
                          >
                            コピー
                          </Button>
                        )}
                      </Space>
                    </Space>
                  }
                  style={{ marginBottom: 16, borderRadius: 6 }}
                >
                  {att.error_message && (
                    <div style={{ color: "#ff4d4f", marginBottom: 8, fontSize: 13 }}>
                      <strong>エラー:</strong> {att.error_message}
                    </div>
                  )}

                  {att.stdout && (
                    <div style={{ marginBottom: 8 }}>
                      <div style={{ fontSize: 12, color: "#8c8c8c", marginBottom: 4 }}>標準出力 (stdout):</div>
                      <pre
                        style={{
                          background: "#1e1e1e",
                          color: "#d4d4d4",
                          padding: 10,
                          borderRadius: 4,
                          fontSize: 12,
                          maxHeight: 180,
                          overflow: "auto",
                          margin: 0,
                        }}
                      >
                        {att.stdout}
                      </pre>
                    </div>
                  )}

                  {att.stderr && (
                    <div>
                      <div style={{ fontSize: 12, color: "#8c8c8c", marginBottom: 4 }}>標準エラー (stderr):</div>
                      <pre
                        style={{
                          background: "#2a1515",
                          color: "#ff8080",
                          padding: 10,
                          borderRadius: 4,
                          fontSize: 12,
                          maxHeight: 180,
                          overflow: "auto",
                          margin: 0,
                        }}
                      >
                        {att.stderr}
                      </pre>
                    </div>
                  )}
                </Card>
              ),
            };
          })}
        />
      )}
    </Drawer>
  );
};
