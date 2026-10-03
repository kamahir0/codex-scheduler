import React from "react";
import { Row, Col, Card, Statistic } from "antd";
import { Clock, Calendar, CheckCircle2, AlertCircle } from "lucide-react";
import { Job, SystemInfo } from "../types";
import dayjs from "dayjs";

interface MetricCardsProps {
  jobs: Job[];
  systemInfo?: SystemInfo | null;
}

export const MetricCards: React.FC<MetricCardsProps> = ({ jobs, systemInfo }) => {
  const activeJobs = jobs.filter((j) => j.status === "scheduled" || j.status === "retrying");
  const runningJobs = jobs.filter((j) => j.status === "running");
  const succeededJobs = jobs.filter((j) => j.status === "succeeded");
  const failedJobs = jobs.filter((j) => j.status === "failed");

  // Find nearest next scheduled job
  const upcomingJobs = activeJobs
    .map((j) => ({ job: j, time: dayjs(j.scheduled_at) }))
    .filter((item) => item.time.isAfter(dayjs()))
    .sort((a, b) => a.time.valueOf() - b.time.valueOf());

  const nextJob = upcomingJobs[0];
  const nextRunStr = nextJob
    ? nextJob.time.format("HH:mm (MM/DD)")
    : "予定なし";

  const nextRunTitle = systemInfo?.os === "macos"
    ? "次回自動実行"
    : "次回予定（参考）";

  return (
    <Row gutter={[16, 16]} style={{ marginBottom: 20 }}>
      <Col xs={24} sm={12} md={6}>
        <Card bordered={false} style={{ borderRadius: 8, cursor: "default" }}>
          <Statistic
            title={
              <span style={{ display: "flex", alignItems: "center", gap: 6, cursor: "default" }}>
                <Clock size={16} color="#1677ff" /> {nextRunTitle}
              </span>
            }
            value={nextRunStr}
            valueStyle={{ fontSize: "1.25rem", fontWeight: 600, cursor: "default" }}
          />
        </Card>
      </Col>
      <Col xs={24} sm={12} md={6}>
        <Card bordered={false} style={{ borderRadius: 8, cursor: "default" }}>
          <Statistic
            title={
              <span style={{ display: "flex", alignItems: "center", gap: 6, cursor: "default" }}>
                <Calendar size={16} /> 待機・実行中ジョブ
              </span>
            }
            value={activeJobs.length + runningJobs.length}
            suffix={
              runningJobs.length > 0
                ? `(実行中: ${runningJobs.length})`
                : "件"
            }
            valueStyle={{ fontWeight: 600, cursor: "default" }}
          />
        </Card>
      </Col>
      <Col xs={24} sm={12} md={6}>
        <Card bordered={false} style={{ borderRadius: 8, cursor: "default" }}>
          <Statistic
            title={
              <span style={{ display: "flex", alignItems: "center", gap: 6, cursor: "default" }}>
                <CheckCircle2 size={16} color="#52c41a" /> 成功完了
              </span>
            }
            value={succeededJobs.length}
            suffix="件"
            valueStyle={{ color: "#52c41a", fontWeight: 600, cursor: "default" }}
          />
        </Card>
      </Col>
      <Col xs={24} sm={12} md={6}>
        <Card bordered={false} style={{ borderRadius: 8, cursor: "default" }}>
          <Statistic
            title={
              <span style={{ display: "flex", alignItems: "center", gap: 6, cursor: "default" }}>
                <AlertCircle size={16} color="#ff4d4f" /> 失敗
              </span>
            }
            value={failedJobs.length}
            suffix="件"
            valueStyle={{ color: failedJobs.length > 0 ? "#ff4d4f" : undefined, fontWeight: 600, cursor: "default" }}
          />
        </Card>
      </Col>
    </Row>
  );
};
