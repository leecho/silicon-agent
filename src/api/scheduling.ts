import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ScheduledTask,
  ScheduledTaskInput,
  TaskExecution,
  ScheduledTaskEvent,
} from "../types";

export async function listScheduledTasks(): Promise<ScheduledTask[]> {
  return await invoke<ScheduledTask[]>("list_scheduled_tasks");
}

export async function getScheduledTask(id: string): Promise<ScheduledTask | null> {
  return await invoke<ScheduledTask | null>("get_scheduled_task", { id });
}

export async function createScheduledTask(input: ScheduledTaskInput): Promise<ScheduledTask> {
  return await invoke<ScheduledTask>("create_scheduled_task", { input });
}

export async function updateScheduledTask(id: string, input: ScheduledTaskInput): Promise<ScheduledTask> {
  return await invoke<ScheduledTask>("update_scheduled_task", { id, input });
}

export async function deleteScheduledTask(id: string, deleteSessions: boolean): Promise<void> {
  await invoke("delete_scheduled_task", { id, deleteSessions });
}

export async function setTaskEnabled(id: string, enabled: boolean): Promise<ScheduledTask> {
  return await invoke<ScheduledTask>("set_task_enabled", { id, enabled });
}

export async function listTaskExecutions(taskId?: string, status?: string): Promise<TaskExecution[]> {
  return await invoke<TaskExecution[]>("list_task_executions", {
    taskId: taskId ?? null,
    status: status ?? null,
  });
}

/** 立即触发一次，返回本次新建的 session id（任务已在运行/触发失败时为 null）。 */
export async function runTaskNow(id: string): Promise<string | null> {
  return await invoke<string | null>("run_task_now", { id });
}

export async function getKeepSystemAwake(): Promise<boolean> {
  return await invoke<boolean>("get_keep_system_awake");
}

export async function setKeepSystemAwake(enabled: boolean): Promise<void> {
  await invoke("set_keep_system_awake", { enabled });
}

export async function subscribeScheduledTaskEvents(
  handler: (event: ScheduledTaskEvent) => void,
): Promise<() => void> {
  return await listen<ScheduledTaskEvent>("scheduled_task_event", (e) => handler(e.payload));
}
