import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export function useProject() {
  const [project, setProject] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadProject();
  }, []);

  const loadProject = async () => {
    try {
      const proj = await invoke("get_current_project");
      setProject(proj);
    } catch (error) {
      console.error("Failed to load project:", error);
    } finally {
      setLoading(false);
    }
  };

  return { project, loading, reload: loadProject };
}
