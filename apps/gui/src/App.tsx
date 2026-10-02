import React, { useState, useEffect, useCallback } from "react";
import {
  ConfigProvider,
  Layout,
  Button,
  Space,
  Typography,
  theme,
  Tag,
  Tooltip,
  Alert,
  message,
} from "antd";
import {
  Plus,
  Moon,
  Sun,
  RefreshCw,
  Cpu,
  ShieldCheck,
} from "lucide-react";
import { Job, CreateJobPayload, SystemInfo } from "./types";
import {
  listJobs,
  createJob,
  cancelJob,
  deleteJob,
  runJobNow,
  getSystemInfo,
} from "./api";
import { MetricCards } from "./components/MetricCards";
import { JobTable } from "./components/JobTable";
import { CreateJobModal } from "./components/CreateJobModal";
import { LogsDrawer } from "./components/LogsDrawer";
import { DiagnosticsModal } from "./components/DiagnosticsModal";

const { Header, Content } = Layout;
const { Title, Text } = Typography;

export const App: React.FC = () => {
  const [isDarkMode, setIsDarkMode] = useState(true);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [selectedJobForLogs, setSelectedJobForLogs] = useState<Job | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);

  const fetchJobs = useCallback(async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      const data = await listJobs();
      setJobs(data);
    } catch (e: any) {
      if (!silent) message.error("ジョブ一覧の取得に失敗しました: " + e.message);
    } finally {
      if (!silent) setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchJobs();
    getSystemInfo().then(setSystemInfo).catch(console.warn);

    // Auto-polling every 5 seconds to sync background worker results
    const timer = setInterval(() => {
      fetchJobs(true);
    }, 5000);

    return () => clearInterval(timer);
  }, [fetchJobs]);

  const handleCreateJob = async (payload: CreateJobPayload) => {
    await createJob(payload);
    await fetchJobs(true);
  };

  const handleRunNow = async (id: string) => {
    message.loading({ content: "ジョブを実行中...", key: "run-job" });
    try {
      await runJobNow(id);
      message.success({ content: "ジョブを実行完了しました", key: "run-job" });
      await fetchJobs(true);
    } catch (e: any) {
      message.error({ content: "実行失敗: " + e.message, key: "run-job" });
    }
  };

  const handleCancelJob = async (id: string) => {
    try {
      await cancelJob(id);
      message.success("ジョブをキャンセルしました");
      await fetchJobs(true);
    } catch (e: any) {
      message.error("キャンセル失敗: " + e.message);
    }
  };

  const handleDeleteJob = async (id: string) => {
    try {
      await deleteJob(id);
      message.success("ジョブを削除しました");
      await fetchJobs(true);
    } catch (e: any) {
      message.error("削除失敗: " + e.message);
    }
  };

  return (
    <ConfigProvider
      theme={{
        algorithm: isDarkMode ? theme.darkAlgorithm : theme.defaultAlgorithm,
        token: {
          colorPrimary: "#1677ff",
          borderRadius: 8,
          fontFamily:
            "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif",
        },
      }}
    >
      <Layout style={{ minHeight: "100vh", background: isDarkMode ? "#141414" : "#f5f5f5" }}>
        <Header
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            background: isDarkMode ? "#1f1f1f" : "#ffffff",
            padding: "0 24px",
            borderBottom: `1px solid ${isDarkMode ? "#303030" : "#f0f0f0"}`,
            height: 64,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <div
              style={{
                width: 34,
                height: 34,
                borderRadius: 8,
                background: "linear-gradient(135deg, #1677ff 0%, #6366f1 100%)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "#fff",
              }}
            >
              <Cpu size={20} />
            </div>
            <div>
              <Title level={4} style={{ margin: 0, lineHeight: 1.2, fontWeight: 700 }}>
                Codex Scheduler
              </Title>
              <Text type="secondary" style={{ fontSize: 11 }}>
                利用枠リセット時 自動再開 & スケジューラ
              </Text>
            </div>
            <Tag color="blue" style={{ marginLeft: 8 }}>
              Tauri 2
            </Tag>
          </div>

          <Space size={12}>
            <Tooltip title="一覧を手動更新">
              <Button
                type="text"
                icon={<RefreshCw size={16} className={loading ? "spin-icon" : ""} />}
                onClick={() => fetchJobs(false)}
              />
            </Tooltip>

            <Tooltip title="システム環境 & 権限診断">
              <Button
                type="text"
                icon={
                  <ShieldCheck
                    size={16}
                    color={systemInfo?.codex_installed ? "#52c41a" : "#faad14"}
                  />
                }
                onClick={() => setDiagnosticsOpen(true)}
              />
            </Tooltip>

            <Tooltip title={isDarkMode ? "ライトモードに切替" : "ダークモードに切替"}>
              <Button
                type="text"
                icon={isDarkMode ? <Sun size={16} /> : <Moon size={16} />}
                onClick={() => setIsDarkMode(!isDarkMode)}
              />
            </Tooltip>

            <Button
              type="primary"
              icon={<Plus size={16} />}
              onClick={() => setModalOpen(true)}
              style={{ fontWeight: 600 }}
            >
              新規ジョブ登録
            </Button>
          </Space>
        </Header>

        <Content style={{ padding: "20px 24px", maxWidth: 1400, margin: "0 auto", width: "100%" }}>
          {systemInfo && !systemInfo.codex_installed && (
            <Alert
              message="OpenAI Codex CLI が検出されませんでした"
              description="夜間の自動再開を実行するには、システムに codex コマンドがインストールされ、PATHが設定されている必要があります。"
              type="warning"
              showIcon
              action={
                <Button size="small" type="primary" onClick={() => setDiagnosticsOpen(true)}>
                  セットアップ診断・手順
                </Button>
              }
              style={{ marginBottom: 16 }}
            />
          )}

          <MetricCards jobs={jobs} />

          <div
            style={{
              background: isDarkMode ? "#1f1f1f" : "#ffffff",
              padding: "20px",
              borderRadius: 10,
              border: `1px solid ${isDarkMode ? "#303030" : "#f0f0f0"}`,
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
              <div>
                <Title level={5} style={{ margin: 0 }}>
                  登録済みジョブ一覧
                </Title>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  OSスケジューラ（launchd / Task Scheduler）により、PCが待機状態でも指定時刻にバックグラウンド実行されます
                </Text>
              </div>
            </div>

            <JobTable
              jobs={jobs}
              loading={loading}
              onViewLogs={(job) => setSelectedJobForLogs(job)}
              onRunNow={handleRunNow}
              onCancelJob={handleCancelJob}
              onDeleteJob={handleDeleteJob}
            />
          </div>
        </Content>

        <CreateJobModal
          open={modalOpen}
          onCancel={() => setModalOpen(false)}
          onSubmit={handleCreateJob}
          systemInfo={systemInfo}
        />

        <DiagnosticsModal
          open={diagnosticsOpen}
          onClose={() => setDiagnosticsOpen(false)}
          systemInfo={systemInfo}
        />

        <LogsDrawer
          job={selectedJobForLogs}
          open={Boolean(selectedJobForLogs)}
          onClose={() => setSelectedJobForLogs(null)}
        />
      </Layout>
    </ConfigProvider>
  );
};
