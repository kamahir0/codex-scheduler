import { Job, CreateJobPayload, SystemInfo } from "./types";

const isTauri = () => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

// Mock in-memory store for browser preview / development
let mockJobs: Job[] = [
  {
    id: "job-demo-001",
    provider: "codex",
    session_id: "019abc-8973-4def-91a2-codex",
    cwd: "/Users/mahirohiratsuka/develop/project-demo",
    prompt: "continue",
    scheduled_at: new Date(Date.now() + 2 * 3600 * 1000).toISOString(),
    status: "scheduled",
    retry_policy: {
      enabled: true,
      interval_seconds: 300,
      max_attempts: 6,
      retry_on_quota_only: true,
    },
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    execution_history: [],
  },
  {
    id: "job-demo-002",
    provider: "codex",
    session_id: "019def-3412-4cba-81b4-codex",
    cwd: "/Users/mahirohiratsuka/develop/tool/codex-scheduler",
    prompt: "continue",
    scheduled_at: new Date(Date.now() - 3600 * 1000).toISOString(),
    status: "succeeded",
    retry_policy: {
      enabled: true,
      interval_seconds: 300,
      max_attempts: 6,
      retry_on_quota_only: true,
    },
    created_at: new Date(Date.now() - 4000 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 3500 * 1000).toISOString(),
    execution_history: [
      {
        attempt_number: 1,
        started_at: new Date(Date.now() - 3600 * 1000).toISOString(),
        finished_at: new Date(Date.now() - 3550 * 1000).toISOString(),
        exit_code: 0,
        stdout: "Codex session resumed successfully.\nPrompt 'continue' processed.\n2 files edited.\nTests passed.",
        stderr: "",
        is_quota_error: false,
        error_message: null,
      },
    ],
  },
];

export async function listJobs(): Promise<Job[]> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Job[]>("list_jobs");
  }
  return [...mockJobs];
}

export async function createJob(payload: CreateJobPayload): Promise<Job> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Job>("create_job", { payload });
  }

  const newJob: Job = {
    id: `job-${Date.now()}`,
    provider: (payload.provider as any) || "codex",
    session_id: payload.session_id,
    cwd: payload.cwd,
    prompt: payload.prompt || "continue",
    scheduled_at: payload.scheduled_at,
    status: "scheduled",
    retry_policy: {
      enabled: payload.retry_enabled,
      interval_seconds: payload.retry_interval_seconds,
      max_attempts: payload.max_attempts,
      retry_on_quota_only: true,
    },
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    execution_history: [],
  };
  mockJobs.push(newJob);
  return newJob;
}

export async function cancelJob(id: string): Promise<Job> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Job>("cancel_job", { id });
  }

  const job = mockJobs.find((j) => j.id === id);
  if (!job) throw new Error("Job not found");
  job.status = "cancelled";
  job.updated_at = new Date().toISOString();
  return job;
}

export async function deleteJob(id: string): Promise<boolean> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<boolean>("delete_job", { id });
  }

  const idx = mockJobs.findIndex((j) => j.id === id);
  if (idx !== -1) {
    mockJobs.splice(idx, 1);
    return true;
  }
  return false;
}

export async function runJobNow(id: string): Promise<Job> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Job>("run_job_now", { id });
  }

  const job = mockJobs.find((j) => j.id === id);
  if (!job) throw new Error("Job not found");
  job.status = "succeeded";
  job.updated_at = new Date().toISOString();
  job.execution_history.push({
    attempt_number: job.execution_history.length + 1,
    started_at: new Date().toISOString(),
    finished_at: new Date().toISOString(),
    exit_code: 0,
    stdout: "Manual execution succeeded: Session resumed with prompt '" + job.prompt + "'",
    stderr: "",
    is_quota_error: false,
    error_message: null,
  });
  return job;
}

export async function pickDirectory(): Promise<string | null> {
  if (isTauri()) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
      });
      return typeof selected === "string" ? selected : null;
    } catch (e) {
      console.warn("Failed to open native directory picker:", e);
      return null;
    }
  }
  return null;
}

export async function getSystemInfo(): Promise<SystemInfo> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<SystemInfo>("get_system_info");
  }
  return {
    os: "macos",
    default_cwd: "/Users/mahirohiratsuka/develop",
    codex_installed: true,
    codex_path: "/opt/homebrew/bin/codex",
    cli_worker_path: "/usr/local/bin/codex-scheduler-cli",
    jobs_store_path: "~/.codex-scheduler/jobs.json",
  };
}
