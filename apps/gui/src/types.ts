export type JobStatus =
  | "scheduled"
  | "running"
  | "retrying"
  | "succeeded"
  | "failed"
  | "cancelled";

export type ProviderType = "codex" | "claude" | { custom: string };

export interface RetryPolicy {
  enabled: boolean;
  interval_seconds: number;
  max_attempts: number;
  retry_on_quota_only: boolean;
}

export interface ExecutionAttempt {
  attempt_number: number;
  started_at: string;
  finished_at: string;
  exit_code: number | null;
  stdout: string;
  stderr: string;
  is_quota_error: boolean;
  error_message: string | null;
}

export interface Job {
  id: string;
  provider: ProviderType;
  session_id: string;
  cwd: string;
  prompt: string;
  scheduled_at: string;
  status: JobStatus;
  retry_policy: RetryPolicy;
  created_at: string;
  updated_at: string;
  execution_history: ExecutionAttempt[];
}

export interface CreateJobPayload {
  provider?: string;
  session_id: string;
  cwd: string;
  prompt?: string;
  scheduled_at: string;
  retry_enabled: boolean;
  retry_interval_seconds: number;
  max_attempts: number;
}

export interface SystemInfo {
  os: string;
  default_cwd: string;
  codex_installed: boolean;
  codex_path: string | null;
  cli_worker_path: string;
  cli_worker_installed: boolean;
  jobs_store_path: string;
}

