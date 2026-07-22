import { useEffect, useState } from "react";

import { listProjects } from "../../api";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Project } from "../../types";
import { ProjectList } from "./ProjectList";
import { ProjectView } from "./ProjectView";

/** 项目页：负责项目列表加载和当前打开项目的顶层切换。 */
export function ProjectsPage({
  onBack,
  onNewScheduledTask,
  onOpenProject,
  onOpenProjectList,
  onOpenScheduledTask,
  projectId,
}: {
  onBack: () => void;
  onNewScheduledTask: (projectId: string) => void;
  onOpenProject: (projectId: string) => void;
  onOpenProjectList: () => void;
  onOpenScheduledTask: (taskId: string) => void;
  projectId?: string | null;
}) {
  const notify = useNotifications();
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [openId, setOpenId] = useState<string | null>(null);

  async function reload() {
    try {
      setProjects(await listProjects());
    } catch (err) {
      notify.notify({ tone: "error", title: "加载项目失败", message: String(err) });
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  useEffect(() => {
    if (projectId !== undefined) {
      setOpenId(projectId);
    }
  }, [projectId]);

  const open = projects.find((p) => p.id === openId) ?? null;
  if (open) {
    return (
      <ProjectView
        project={open}
        onBack={() => {
          onBack();
          void reload();
        }}
        onNewScheduledTask={onNewScheduledTask}
        onOpenScheduledTask={onOpenScheduledTask}
        onReload={() => void reload()}
      />
    );
  }

  return (
    <ProjectList
      projects={projects}
      loading={loading}
      onOpenProject={onOpenProject}
      onCreated={(p) => {
        void reload();
        onOpenProject(p.id);
      }}
      onReload={() => {
        onOpenProjectList();
        void reload();
      }}
    />
  );
}
