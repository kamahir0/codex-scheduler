import React, { useState, useEffect, useCallback } from "react";
import {
  ConfigProvider,
  Layout,
  Button,
  Space,
  Typography,
  theme,
  Tooltip,
  Alert,
  message,
  Dropdown,
} from "antd";
import type { MenuProps } from "antd";
import {
  Plus,
  Moon,
  Sun,
  Monitor,
  RefreshCw,
  ShieldCheck,
  Check,
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

type ThemeMode = "system" | "light" | "dark";

export const App: React.FC = () => {
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => {
    if (typeof window !== "undefined") {
      const saved = localStorage.getItem("codex_scheduler_theme");
      if (saved === "light" || saved === "dark" || saved === "system") {
        return saved;
      }
    }
    return "system";
  });

  const [systemPrefersDark, setSystemPrefersDark] = useState(() => {
    if (typeof window !== "undefined" && window.matchMedia) {
      return window.matchMedia("(prefers-color-scheme: dark)").matches;
    }
    return true;
  });

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => setSystemPrefersDark(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  const isDarkMode = themeMode === "system" ? systemPrefersDark : themeMode === "dark";

  const handleThemeChange = (mode: ThemeMode) => {
    setThemeMode(mode);
    localStorage.setItem("codex_scheduler_theme", mode);
  };

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

  const themeMenuItems: MenuProps["items"] = [
    {
      key: "system",
      icon: <Monitor size={15} />,
      label: (
        <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "space-between", minWidth: 150 }}>
          システム設定に従う
          {themeMode === "system" && <Check size={14} color="#1677ff" style={{ marginLeft: 8 }} />}
        </span>
      ),
      onClick: () => handleThemeChange("system"),
    },
    {
      key: "light",
      icon: <Sun size={15} />,
      label: (
        <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "space-between", minWidth: 150 }}>
          ライトモード
          {themeMode === "light" && <Check size={14} color="#1677ff" style={{ marginLeft: 8 }} />}
        </span>
      ),
      onClick: () => handleThemeChange("light"),
    },
    {
      key: "dark",
      icon: <Moon size={15} />,
      label: (
        <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "space-between", minWidth: 150 }}>
          ダークモード
          {themeMode === "dark" && <Check size={14} color="#1677ff" style={{ marginLeft: 8 }} />}
        </span>
      ),
      onClick: () => handleThemeChange("dark"),
    },
  ];

  const getCurrentThemeIcon = () => {
    if (themeMode === "system") {
      return <Monitor size={16} />;
    }
    return themeMode === "dark" ? <Moon size={16} /> : <Sun size={16} />;
  };

  const getCurrentThemeLabel = () => {
    if (themeMode === "system") return "システム追従";
    return themeMode === "dark" ? "ダークモード" : "ライトモード";
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
      <Layout
        style={{
          height: "100vh",
          display: "flex",
          flexDirection: "column",
          background: isDarkMode ? "#141414" : "#f5f5f5",
          cursor: "default",
          overflow: "hidden",
        }}
      >
        <Header
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            background: isDarkMode ? "#1f1f1f" : "#ffffff",
            padding: "0 24px",
            borderBottom: `1px solid ${isDarkMode ? "#303030" : "#f0f0f0"}`,
            height: 64,
            lineHeight: "normal",
            boxSizing: "border-box",
            cursor: "default",
            userSelect: "none",
            flexShrink: 0,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 12, height: "100%" }}>
            <img
              src="/app-icon.png"
              alt="Codex Scheduler Logo"
              style={{
                width: 36,
                height: 36,
                borderRadius: 8,
                objectFit: "contain",
                flexShrink: 0,
                boxShadow: isDarkMode ? "0 2px 6px rgba(0,0,0,0.5)" : "0 2px 6px rgba(0,0,0,0.12)",
                cursor: "default",
              }}
            />
            <div style={{ display: "flex", flexDirection: "column", justifyContent: "center" }}>
              <div
                style={{
                  fontSize: 16,
                  fontWeight: 700,
                  lineHeight: 1.25,
                  color: isDarkMode ? "#ffffff" : "#1f1f1f",
                  letterSpacing: "-0.01em",
                }}
              >
                Codex Scheduler
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: isDarkMode ? "#8c8c8c" : "#8c8c8c",
                  lineHeight: 1.2,
                  marginTop: 2,
                }}
              >
                利用枠リセット時 自動再開 & スケジューラ
              </div>
            </div>
          </div>

          <Space size={12} align="center">
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

            <Dropdown
              menu={{ items: themeMenuItems, selectedKeys: [themeMode] }}
              trigger={["hover"]}
              placement="bottomRight"
            >
              <Button
                type="text"
                aria-label="テーマ設定"
                icon={getCurrentThemeIcon()}
              />
            </Dropdown>

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

        <Content
          style={{
            padding: "20px 24px",
            maxWidth: 1400,
            margin: "0 auto",
            width: "100%",
            boxSizing: "border-box",
            cursor: "default",
            flex: 1,
            display: "flex",
            flexDirection: "column",
            minHeight: 0,
            overflowY: "auto",
          }}
        >
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
              style={{ marginBottom: 16, flexShrink: 0 }}
            />
          )}

          <div style={{ flexShrink: 0 }}>
            <MetricCards jobs={jobs} systemInfo={systemInfo} />
          </div>

          <div
            style={{
              background: isDarkMode ? "#1f1f1f" : "#ffffff",
              padding: "20px",
              borderRadius: 10,
              border: `1px solid ${isDarkMode ? "#303030" : "#f0f0f0"}`,
              cursor: "default",
              flex: 1,
              display: "flex",
              flexDirection: "column",
              minHeight: 0,
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                marginBottom: 16,
                userSelect: "none",
                cursor: "default",
                flexShrink: 0,
              }}
            >
              <div>
                <Title level={5} style={{ margin: 0, cursor: "default" }}>
                  登録済みジョブ一覧
                </Title>
                <Text type="secondary" style={{ fontSize: 12, cursor: "default" }}>
                  {systemInfo?.os === "macos"
                    ? "OSスケジューラ（launchd LaunchAgent）により、PCが待機状態でも指定時刻以降（通常1分以内）にバックグラウンド実行されます"
                    : "Windows環境ではTask Scheduler常設連携は未対応です（指定時刻の自動バックグラウンド実行は行われません。現行バージョンではジョブ管理・手動実行用となります）"}
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
