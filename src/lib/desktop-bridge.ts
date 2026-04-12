// @ts-nocheck
import type { DesktopPlatform } from "@/types/app";
import type {
  NativeNotificationActionEvent,
  NativeNotificationPayload,
} from "@/types/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const isTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const platformFromUA = (): DesktopPlatform => {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("mac")) return "darwin";
  if (ua.includes("win")) return "win32";
  if (ua.includes("linux")) return "linux";
  return "linux";
};

const call = <T>(cmd: string, args?: Record<string, unknown>) =>
  invoke<T>(cmd, args ?? {});

/** Expõe `window.desktopAPI` e `window.db` via Tauri `invoke` (paridade com o antigo preload). */
export function installDesktopBridge(): void {
  if (typeof window === "undefined") return;
  /** Só pular quando o SQLite já está exposto; `desktopAPI` pode existir sem `db` (stub Tauri) e isso bloqueava o bridge. */
  if (window.db) return;
  if (!isTauriRuntime()) return;

  window.desktopAPI = {
    platform: platformFromUA(),
    app: {
      getVersion: () => call<string>("app_get_version"),
      checkForUpdates: () => call<void>("app_check_for_updates"),
      quitAndInstall: () => call<void>("app_quit_and_install"),
      showNotification: (payload: NativeNotificationPayload) =>
        call<{ ok?: boolean; notificationId?: string }>("app_show_notification", { payload }),
      onUpdateStatus: (_callback) => () => {},
      onNotificationAction: (callback) => {
        let unlistenFn: (() => void) | null = null;
        void listen("notification-action", (event: any) => {
          const payload = event?.payload ?? {};
          callback(payload as NativeNotificationActionEvent);
        })
          .then((unlisten) => {
            unlistenFn = unlisten;
          })
          .catch((err) => {
            console.warn("[desktop-bridge] app onNotificationAction listen failed:", err);
          });
        return () => {
          if (unlistenFn) unlistenFn();
        };
      },
    },
    dialog: {
      selectDirectory: () => call<string | null>("dialog_select_directory"),
      showMessage: (options) => call<number>("dialog_show_message", { options }),
      confirm: (message) => call<boolean>("dialog_confirm", { message }),
    },
    shell: {
      openExternal: (url) => call<void>("shell_open_external", { url }),
      openPath: (path) => call<void>("shell_open_path", { path }),
      showItemInFolder: (path) => call<void>("shell_show_item_in_folder", { path }),
      resolveCliPath: (command) =>
        call<{ path: string | null }>("shell_resolve_cli_path", { command }),
      detectCliForProvider: (providerType) =>
        call<{ path: string | null }>("shell_detect_cli_for_provider", {
          providerType,
        }),
      validateCliPath: (providerType, cliPath) =>
        call<{ valid: boolean; message?: string }>("shell_validate_cli_path", {
          providerType,
          cliPath,
        }),
      openTerminalAtPath: (dirPath, suggestedCommand) =>
        call<{ success: boolean; error?: string }>("shell_open_terminal_at_path", {
          dirPath,
          suggestedCommand,
        }),
    },
    terminal: {
      spawn: (options) => call("terminal_spawn", { options }),
      getOrCreate: (missionId, options) =>
        call("terminal_get_or_create", { missionId, options }),
      getSession: (missionId) => call("terminal_get_session", { missionId }),
      write: (ptyId, data) => call("terminal_write", { ptyId, data }),
      resize: (ptyId, cols, rows) => call("terminal_resize", { ptyId, cols, rows }),
      sendSignal: (ptyId, signal) =>
        call<{ ok: boolean; error?: string }>("terminal_send_signal", { ptyId, signal }),
      kill: (ptyId) => call("terminal_kill", { ptyId }),
      killByMissionId: (missionId) => call("terminal_kill_by_mission_id", { missionId }),
      getOrCreateForPane: (paneId, options) =>
        call("terminal_get_or_create_for_pane", { paneId, options }),
      getPaneSession: (paneId) => call("terminal_get_pane_session", { paneId }),
      killByPaneId: (paneId) => call("terminal_kill_by_pane_id", { paneId }),
      getBacklog: (ptyId) =>
        call<{ lines: string[] }>("terminal_get_backlog", { ptyId }),
      clearPersistedScrollback: (paneId: string) =>
        call<{ ok: boolean }>("terminal_clear_persisted_scrollback", { paneId }),
      getProjectActivity: (projectId) =>
        call<{
          totalRunningPanes: number;
          runningPanesByCombId: Record<string, number>;
        }>("terminal_get_project_activity", { projectId }),
      saveTempImage: (imageData: number[], extension: string) =>
        call<{ path: string; filename: string }>("terminal_save_temp_image", { imageData, extension }),
      onData: (callback) => {
        let unlistenFn: (() => void) | null = null;
        void listen("terminal-output", (event: any) => {
          const payload = event?.payload ?? {};
          callback(payload.ptyId, payload.data ?? "");
        })
          .then((unlisten) => {
            unlistenFn = unlisten;
          })
          .catch((err) => {
            console.warn("[desktop-bridge] terminal onData listen failed:", err);
          });
        return () => {
          if (unlistenFn) unlistenFn();
        };
      },
      onExit: (callback) => {
        let unlistenFn: (() => void) | null = null;
        void listen("terminal-exit", (event: any) => {
          const payload = event?.payload ?? {};
          callback(payload.ptyId, Number(payload.code ?? 0));
        })
          .then((unlisten) => {
            unlistenFn = unlisten;
          })
          .catch((err) => {
            console.warn("[desktop-bridge] terminal onExit listen failed:", err);
          });
        return () => {
          if (unlistenFn) unlistenFn();
        };
      },
      onAttention: (callback) => {
        let unlistenFn: (() => void) | null = null;
        void listen("terminal-attention", (event: any) => {
          const payload = event?.payload ?? {};
          callback(payload);
        })
          .then((unlisten) => {
            unlistenFn = unlisten;
          })
          .catch((err) => {
            console.warn("[desktop-bridge] terminal onAttention listen failed:", err);
          });
        return () => {
          if (unlistenFn) unlistenFn();
        };
      },
    },
    daemon: {
      getStatus: () => call("daemon_get_status"),
      listTasks: () => call("daemon_list_tasks"),
      listProcesses: (projectId) => call("daemon_list_processes", { projectId }),
      startProcess: (projectId, processId) =>
        call("daemon_start_process", { projectId, processId }),
      stopProcess: (projectId, processId) =>
        call("daemon_stop_process", { projectId, processId }),
      restartProcess: (projectId, processId) =>
        call("daemon_restart_process", { projectId, processId }),
      listCombs: (projectId) => call("daemon_list_combs", { projectId }),
      listPanes: (projectId, combId) =>
        call("daemon_list_panes", { projectId, combId }),
      getDiffsBundle: (worktreePaths, combIds) =>
        call("daemon_get_diffs_bundle", { worktreePaths, combIds }),
      runTask: (projectId, taskId) =>
        call("daemon_run_task", { projectId, taskId }),
      attachTask: (projectId, taskId) =>
        call("daemon_attach_task", { projectId, taskId }),
      detachTask: (projectId, taskId) =>
        call("daemon_detach_task", { projectId, taskId }),
    },
    worktree: {
      ensureForMission: (missionId) =>
        call("worktree_ensure_for_mission", { missionId }),
      mergeIntoMain: (missionId) =>
        call("worktree_merge_into_main", { missionId }),
      discard: (missionId) => call("worktree_discard", { missionId }),
      getDiffs: (missionId) => call("missions_get_diffs", { missionId }),
      applyMissionPatch: (missionId, targetBranch, options) =>
        call("worktree_apply_mission_patch", { missionId, targetBranch, options }),
    },
    comb: {
      ensureWorktree: (combId) => call("comb_ensure_worktree", { combId }),
      discard: (combId) => call("comb_discard", { combId }),
      checkUnpushed: (combId) => call<{ hasUnpushed: boolean; count: number; commits: string[] }>("comb_check_unpushed", { combId }),
      mergeIntoMain: (combId, targetBranch) =>
        call("comb_merge_into_main", { combId, targetBranch }),
      getDiffs: (combId) => call("comb_get_diffs", { combId }),
      applyPatch: (combId, targetBranch, options) =>
        call("comb_apply_patch", { combId, targetBranch, options }),
    },
    repo: {
      listTaskTemplates: (projectPath: string) =>
        call<
          Array<{
            id: string;
            name: string;
            command: string;
            description?: string | null;
            cwdMode: "project" | "worktree";
          }>
        >("repo_list_task_templates", { projectPath }),
    },
    window: {
      minimize: () => call<void>("window_minimize"),
      maximize: () => call<void>("window_maximize"),
      close: () => call<void>("window_close"),
      isMaximized: () => call<boolean>("window_is_maximized"),
    },
    license: {
      getStatus: () => call("license_get_status"),
      getMachineId: () => call("license_get_machine_id"),
      activate: (email) => call("license_activate", { email }),
      skipActivation: () => call("license_skip_activation"),
    },
    ai: {
      generatePlan: (missionId, options) =>
        call("ai_generate_plan", { missionId, options }),
      generateCode: (missionId, options) =>
        call("ai_generate_code", { missionId, options }),
      applyChanges: (missionId, options) =>
        call("ai_apply_changes", { missionId, options }),
      testConnection: (providerId) => call("ai_test_connection", { providerId }),
      validateProvider: (provider) => call("ai_validate_provider", { provider }),
      invalidateAdapter: (providerId) => call("ai_invalidate_adapter", { providerId }),
    },
    git: {
      getInfo: (projectPath) => call("git_get_info", { projectPath }),
      getStatus: (projectPath) => call("git_get_status", { projectPath }),
      getBranchState: (projectPath) => call("git_get_branch_state", { projectPath }),
      getFileDiffHead: (projectPath, filePath) =>
        call("git_get_file_diff_head", { projectPath, filePath }),
      getFileDiffAgainstBase: (projectPath, filePath, baseRef) =>
        call("git_get_file_diff_against_base", { projectPath, filePath, baseRef }),
      isRepo: (projectPath) => call("git_is_repo", { projectPath }),
      getCurrentBranch: (projectPath) => call("git_get_current_branch", { projectPath }),
      getDefaultBranch: (projectPath) => call("git_get_default_branch", { projectPath }),
      getLocalBranches: (projectPath) => call("git_get_local_branches", { projectPath }),
      createBranch: (projectPath, branchName, fromBranch) =>
        call("git_create_branch", { projectPath, branchName, fromBranch }),
      listFiles: (projectPath, maxFiles) => call("git_list_files", { projectPath, maxFiles }),
      getRecentCommits: (projectPath, count) =>
        call("git_get_recent_commits", { projectPath, count }),
      commit: (projectPath, message, files) =>
        call("git_commit", { projectPath, message, files }),
      push: (projectPath) => call("git_push", { projectPath }),
      pull: (projectPath) => call("git_pull", { projectPath }),
      reset: (projectPath, ref) =>
        call("git_reset", { projectPath, gitRef: ref }),
      getWorktreeInfo: (projectPath) => call("git_get_worktree_info", { projectPath }),
      getReviewDiffs: (worktreePath) => call("git_get_review_diffs", { worktreePath }),
      applyWorktreePatch: (mainProjectPath, worktreePath, targetBranch, options) =>
        call("git_apply_worktree_patch", {
          mainProjectPath,
          worktreePath,
          targetBranch,
          options,
        }),
    },
    review: {
      getDiffsBundle: (worktreePaths) => call("review_get_diffs_bundle", { worktreePaths }),
    },
  } as any;

  window.db = {
    providers: {
      findAll: () => call("db_providers_find_all"),
      findById: (id) => call("db_providers_find_by_id", { id }),
      findByType: (type) => call("db_providers_find_by_type", { kind: type }),
      findActive: () => call("db_providers_find_active"),
      create: (data) => call("db_providers_create", { data }),
      update: (id, data) => call("db_providers_update", { id, data }),
      delete: (id) => call("db_providers_delete", { id }),
      setActive: (id, isActive) => call("db_providers_set_active", { id, isActive }),
      testConnection: (id) => call("db_providers_test_connection", { id }),
      isEncryptionAvailable: () => call("db_providers_is_encryption_available"),
    },
    projects: {
      findAll: () => call("db_projects_find_all"),
      findById: (id) => call("db_projects_find_by_id", { id }),
      findByPath: (path) => call("db_projects_find_by_path", { path }),
      getRepoConfigToml: (id) =>
        call<{
          exists: boolean;
          source: "disk" | "db" | "generated" | "missing";
          path: string | null;
          content: string;
        }>("db_projects_get_repo_config_toml", { id }),
      saveRepoConfigToml: (id, content) =>
        call<{ success: boolean; path?: string; error?: string }>(
          "db_projects_save_repo_config_toml",
          { id, content },
        ),
      search: (query) => call("db_projects_search", { query }),
      create: (data) => call("db_projects_create", { data }),
      update: (id, data) => call("db_projects_update", { id, data }),
      delete: (id) => call("db_projects_delete", { id }),
      getStats: (id) => call("db_projects_get_stats", { id }),
      updateLastOpened: (id) => call("db_projects_update_last_opened", { id }),
    },
    combs: {
      findByProject: (projectId) => call("db_combs_find_by_project", { projectId }),
      findById: (id) => call("db_combs_find_by_id", { id }),
      create: (data) => call("db_combs_create", { data }),
      update: (id, data) => call("db_combs_update", { id, data }),
      delete: (id) => call("db_combs_delete", { id }),
      togglePin: async (id: string) => {
        const comb = await call<{ isPinned?: boolean } | null>("db_combs_find_by_id", {
          id,
        });
        if (comb == null || comb === undefined) return undefined;
        const newPinned = !comb.isPinned;
        return call("db_combs_update", {
          id,
          data: {
            isPinned: newPinned,
            pinnedAt: newPinned ? new Date().toISOString() : null,
          },
        });
      },
    },
    panes: {
      findByComb: (combId) => call("db_panes_find_by_comb", { combId }),
      findById: (id) => call("db_panes_find_by_id", { id }),
      create: (data) => call("db_panes_create", { data }),
      update: (id, data) => call("db_panes_update", { id, data }),
      delete: (id) => call("db_panes_delete", { id }),
    },
    utils: {
      backup: (destPath) => call("db_utils_backup", { destPath }),
      getPath: () => call("db_utils_get_path"),
      getSize: () => call("db_utils_get_size"),
    },
  } as any;
}
