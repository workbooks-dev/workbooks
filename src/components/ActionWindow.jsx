import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export function ActionWindow({ onAction }) {
  const [recentProjects, setRecentProjects] = useState([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadRecentProjects();
  }, []);

  async function loadRecentProjects() {
    try {
      const projects = await invoke("get_recent_projects");
      setRecentProjects(projects);
    } catch (error) {
      console.error("Failed to load recent projects:", error);
    } finally {
      setLoading(false);
    }
  }

  async function handleCreateProject() {
    onAction?.({ type: "create-project" });
  }

  async function handleOpenProject() {
    try {
      const folderPath = await open({
        directory: true,
        multiple: false,
        title: "Open Project",
      });

      if (folderPath) {
        onAction?.({ type: "open-project", path: folderPath });
      }
    } catch (error) {
      console.error("Failed to open project:", error);
    }
  }

  function handleRecentProjectClick(project) {
    onAction?.({ type: "open-project", path: project.path });
  }

  function handleViewAllRuns() {
    onAction?.({ type: "view-all-runs" });
  }

  function handleViewAllSchedules() {
    onAction?.({ type: "view-all-schedules" });
  }

  function handleOpenSettings() {
    onAction?.({ type: "open-settings" });
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen bg-gray-50">
        <p className="text-sm text-gray-500">Loading...</p>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-center min-h-screen bg-gray-50 p-8">
      <div className="w-full max-w-2xl">
        {/* Logo/Header */}
        <div className="text-center mb-12">
          <h1 className="text-3xl font-semibold text-gray-900 mb-2">Tether</h1>
          <p className="text-sm text-gray-600">
            Sharpen your automations
          </p>
        </div>

        {/* Main Content Card */}
        <div className="bg-white border border-gray-200 rounded-lg shadow-sm">
          {/* Recent Projects Section */}
          {recentProjects.length > 0 && (
            <div className="px-6 py-5 border-b border-gray-200">
              <h2 className="text-sm font-semibold text-gray-900 mb-3 uppercase tracking-wider">
                Recent Projects
              </h2>
              <div className="space-y-1">
                {recentProjects.map((project, index) => (
                  <button
                    key={index}
                    onClick={() => handleRecentProjectClick(project)}
                    className="w-full px-4 py-3 text-left rounded-md hover:bg-gray-50 transition-colors group"
                  >
                    <div className="font-medium text-gray-900 group-hover:text-blue-600 transition-colors">
                      {project.name}
                    </div>
                    <div className="text-xs text-gray-500 mt-0.5">
                      {project.path}
                    </div>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Projects Section */}
          <div className="px-6 py-5 border-b border-gray-200">
            <h2 className="text-sm font-semibold text-gray-900 mb-3 uppercase tracking-wider">
              Projects
            </h2>
            <div className="grid grid-cols-2 gap-3">
              <button
                onClick={handleCreateProject}
                className="px-4 py-3 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors shadow-sm"
              >
                Create Project
              </button>
              <button
                onClick={handleOpenProject}
                className="px-4 py-3 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
              >
                Open Project...
              </button>
            </div>
          </div>

          {/* Global Views Section */}
          <div className="px-6 py-5 border-b border-gray-200">
            <h2 className="text-sm font-semibold text-gray-900 mb-3 uppercase tracking-wider">
              Global Views
            </h2>
            <div className="grid grid-cols-2 gap-3">
              <button
                onClick={handleViewAllRuns}
                className="px-4 py-3 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
              >
                View All Runs
              </button>
              <button
                onClick={handleViewAllSchedules}
                className="px-4 py-3 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors"
              >
                View All Schedules
              </button>
            </div>
          </div>

          {/* Settings Section */}
          <div className="px-6 py-5">
            <h2 className="text-sm font-semibold text-gray-900 mb-3 uppercase tracking-wider">
              Settings
            </h2>
            <button
              onClick={handleOpenSettings}
              className="w-full px-4 py-3 text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 border border-gray-300 rounded-md transition-colors flex items-center justify-center gap-2"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
              App Settings
            </button>
          </div>
        </div>

        {/* Empty state for no recent projects */}
        {recentProjects.length === 0 && (
          <div className="mt-6 text-center">
            <p className="text-sm text-gray-500">
              No recent projects yet. Create or open a project to get started.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
