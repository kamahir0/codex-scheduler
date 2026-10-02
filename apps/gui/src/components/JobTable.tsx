import React from "react";
import { Table, Tag, Badge, Space, Button, Popconfirm, Tooltip, Typography } from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  Play,
  FileText,
  Trash2,
  XCircle,
  Copy,
  Clock,
  Folder,
  RefreshCw,
} from "lucide-react";
import { Job, JobStatus } from "../types";
import dayjs from "dayjs";

const { Text } = Typography;

interface JobTableProps {
  jobs: Job[];
  loading: boolean;
  onViewLogs: (job: Job) => void;
  onRunNow: (id: string) => Promise<void>;
  onCancelJob: (id: string) => Promise<void>;
  onDeleteJob: (id: string) => Promise<void>;
}

export const JobTable: React.FC<JobTableProps> = ({
  jobs,
  loading,
  onViewLogs,
  onRunNow,
  onCancelJob,
  onDeleteJob,
}) => {
  const getStatusBadge = (status: JobStatus) => {
    switch (status) {
      case "scheduled":
        return <Badge status="processing" text={<Tag color="blue">待機中 (Scheduled)</Tag>} />;
      case "running":
        return (
          <Badge
            status="warning"
            text={
              <Tag color="gold" icon={<RefreshCw size={12} className="spin-icon" />}>
                実行中 (Running)
              </Tag>
            }
          />
        );
      case "retrying":
        return <Badge status="warning" text={<Tag color="orange">リトライ待機中</Tag>} />;
      case "succeeded":
        return <Badge status="success" text={<Tag color="green">成功 (Succeeded)</Tag>} />;
      case "failed":
        return <Badge status="error" text={<Tag color="red">失敗 (Failed)</Tag>} />;
      case "cancelled":
        return <Badge status="default" text={<Tag color="default">取消済 (Cancelled)</Tag>} />;
    }
  };

  const columns: ColumnsType<Job> = [
    {
      title: "ステータス",
      dataIndex: "status",
      key: "status",
      width: 170,
      render: (status: JobStatus) => getStatusBadge(status),
      filters: [
        { text: "待機中", value: "scheduled" },
        { text: "実行中", value: "running" },
        { text: "リトライ中", value: "retrying" },
        { text: "成功", value: "succeeded" },
        { text: "失敗", value: "failed" },
      ],
      onFilter: (value, record) => record.status === value,
    },
    {
      title: "セッション ID / プロバイダ",
      key: "session",
      width: 220,
      render: (_, record) => {
        const providerName =
          typeof record.provider === "string"
            ? record.provider.toUpperCase()
            : "CUSTOM";
        const shortId =
          record.session_id.length > 14
            ? `${record.session_id.substring(0, 12)}...`
            : record.session_id;

        return (
          <Space direction="vertical" size={2}>
            <Space size={6}>
              <Tag color="cyan" style={{ fontSize: 11, padding: "0 6px" }}>
                {providerName}
              </Tag>
              <Text copyable={{ text: record.session_id }} style={{ fontSize: 13, fontFamily: "monospace" }}>
                {shortId}
              </Text>
            </Space>
          </Space>
        );
      },
    },
    {
      title: "作業ディレクトリ",
      dataIndex: "cwd",
      key: "cwd",
      ellipsis: true,
      render: (cwd: string) => {
        const parts = cwd.split("/");
        const display = parts.length > 3 ? `.../${parts.slice(-2).join("/")}` : cwd;
        return (
          <Tooltip title={cwd}>
            <span style={{ display: "inline-flex", alignItems: "center", gap: 5, fontSize: 13, color: "#8c8c8c" }}>
              <Folder size={14} color="#1677ff" />
              <span>{display}</span>
            </span>
          </Tooltip>
        );
      },
    },
    {
      title: "指示プロンプト",
      dataIndex: "prompt",
      key: "prompt",
      width: 120,
      render: (prompt: string) => <Tag color="geekblue">{prompt}</Tag>,
    },
    {
      title: "実行予定日時",
      dataIndex: "scheduled_at",
      key: "scheduled_at",
      width: 210,
      sorter: (a, b) => dayjs(a.scheduled_at).valueOf() - dayjs(b.scheduled_at).valueOf(),
      defaultSortOrder: "ascend",
      render: (scheduled_at: string) => {
        const target = dayjs(scheduled_at);
        const now = dayjs();
        const diffMins = target.diff(now, "minute");
        const diffHours = target.diff(now, "hour");

        let relBadge = "";
        if (diffMins < 0) {
          relBadge = "過去";
        } else if (diffHours > 24) {
          relBadge = `あと${target.diff(now, "day")}日`;
        } else if (diffHours > 0) {
          relBadge = `あと約${diffHours}時間`;
        } else {
          relBadge = `あと${diffMins}分`;
        }

        return (
          <div>
            <div style={{ fontWeight: 500 }}>{target.format("YYYY-MM-DD HH:mm")}</div>
            <div style={{ fontSize: 12, color: diffMins > 0 ? "#1677ff" : "#8c8c8c", display: "flex", alignItems: "center", gap: 4 }}>
              <Clock size={12} />
              <span>{relBadge}</span>
            </div>
          </div>
        );
      },
    },
    {
      title: "試行状況",
      key: "attempts",
      width: 110,
      render: (_, record) => {
        const current = record.execution_history.length;
        const max = record.retry_policy.max_attempts;
        return (
          <Text style={{ fontSize: 13 }}>
            {current} / {max} 回
          </Text>
        );
      },
    },
    {
      title: "操作",
      key: "actions",
      width: 170,
      fixed: "right",
      render: (_, record) => {
        const canCancel = record.status === "scheduled" || record.status === "retrying";
        return (
          <Space size={4}>
            <Tooltip title="実行ログ・詳細">
              <Button
                type="text"
                size="small"
                icon={<FileText size={15} />}
                onClick={() => onViewLogs(record)}
              />
            </Tooltip>

            <Tooltip title="今すぐ手動実行">
              <Button
                type="text"
                size="small"
                icon={<Play size={15} color="#52c41a" />}
                onClick={() => onRunNow(record.id)}
              />
            </Tooltip>

            {canCancel && (
              <Popconfirm
                title="ジョブをキャンセルしますか？"
                description="OSスケジューラからの登録が解除されます。"
                onConfirm={() => onCancelJob(record.id)}
                okText="キャンセルする"
                cancelText="戻る"
              >
                <Tooltip title="スケジュール取消">
                  <Button type="text" size="small" icon={<XCircle size={15} color="#faad14" />} />
                </Tooltip>
              </Popconfirm>
            )}

            <Popconfirm
              title="ジョブを完全に削除しますか？"
              onConfirm={() => onDeleteJob(record.id)}
              okText="削除"
              cancelText="戻る"
              okButtonProps={{ danger: true }}
            >
              <Tooltip title="削除">
                <Button type="text" danger size="small" icon={<Trash2 size={15} />} />
              </Tooltip>
            </Popconfirm>
          </Space>
        );
      },
    },
  ];

  return (
    <Table
      columns={columns}
      dataSource={jobs}
      rowKey="id"
      loading={loading}
      pagination={{ pageSize: 8, showSizeChanger: false }}
      style={{ borderRadius: 8, overflow: "hidden" }}
      scroll={{ x: 960 }}
    />
  );
};
