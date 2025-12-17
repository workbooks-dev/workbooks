import { invoke } from "@tauri-apps/api/core";

export function useTether() {
  const listNotebooks = async () => {
    return invoke("list_notebooks");
  };

  const runNotebook = async (path) => {
    return invoke("run_notebook", { path });
  };

  const getState = async () => {
    return invoke("get_state");
  };

  const inspectStateVariable = async (key) => {
    return invoke("inspect_state_variable", { key });
  };

  const getRunLogs = async () => {
    return invoke("get_run_logs");
  };

  return {
    listNotebooks,
    runNotebook,
    getState,
    inspectStateVariable,
    getRunLogs,
  };
}
