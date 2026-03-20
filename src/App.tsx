import { useEffect, useState } from "react";
import { Routes, Route, Outlet } from "react-router-dom";
import { ThemeProvider } from "@/components/theme-provider";
import { ConfirmDialogProvider } from "@/components/providers/confirm-dialog-provider";
import { Toaster } from "@/components/ui/sonner";
import { AppSidebar } from "@/components/app-sidebar";

// Pages
import HiveWorkspacePage from "@/src/pages/HiveWorkspacePage";
import HomePage from "@/src/pages/HomePage";
import SettingsPage from "@/src/pages/SettingsPage";
import ProjectWorkspacePage from "@/src/pages/ProjectWorkspacePage";
import ProjectWorkspaceIndexRedirect from "@/src/pages/ProjectWorkspaceIndexRedirect";
import ProjectPipelinePage from "@/src/pages/ProjectPipelinePage";
import ProjectAgentsPage from "@/src/pages/ProjectAgentsPage";
import ProjectReviewPage from "@/src/pages/ProjectReviewPage";
import MissionPage from "@/src/pages/MissionPage";
import TaskPage from "@/src/pages/TaskPage";
import ActivationPage from "@/src/pages/ActivationPage";

const isElectron = typeof window !== "undefined" && !!window.electronAPI;

function LegacyShell() {
  return (
    <div className="flex h-screen overflow-hidden bg-background">
      <div className="electron-drag fixed top-0 left-0 right-0 h-8 z-50" />
      <AppSidebar />
      <main className="flex-1 overflow-auto">
        <Outlet />
      </main>
    </div>
  );
}

function AppContent() {
  const [licenseLoading, setLicenseLoading] = useState(isElectron);
  const [isActivated, setIsActivated] = useState(false);

  useEffect(() => {
    if (!isElectron || !window.electronAPI?.license) {
      setLicenseLoading(false);
      setIsActivated(true);
      return;
    }
    window.electronAPI.license
      .getStatus()
      .then((status) => {
        setIsActivated(!!status?.activated);
      })
      .catch(() => setIsActivated(false))
      .finally(() => setLicenseLoading(false));
  }, []);

  if (isElectron && licenseLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  if (isElectron && !isActivated) {
    return <ActivationPage onActivated={() => setIsActivated(true)} />;
  }

  return (
    <Routes>
      {/* Hive workspace — full-screen, own sidebar, no AppSidebar */}
      <Route path="/" element={<HiveWorkspacePage />} />

      {/* Legacy routes — wrapped in the classic AppSidebar shell */}
      <Route element={<LegacyShell />}>
        <Route path="/projects" element={<HomePage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/project/:id" element={<ProjectWorkspacePage />}>
          <Route index element={<ProjectWorkspaceIndexRedirect />} />
          <Route path="pipeline" element={<ProjectPipelinePage />} />
          <Route path="agents" element={<ProjectAgentsPage />} />
          <Route path="review" element={<ProjectReviewPage />} />
        </Route>
        <Route
          path="/project/:id/mission/:missionId"
          element={<MissionPage />}
        />
        <Route
          path="/project/:id/task/:missionId"
          element={<TaskPage />}
        />
      </Route>
    </Routes>
  );
}

export default function App() {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="dark"
      enableSystem
      disableTransitionOnChange
    >
      <ConfirmDialogProvider>
        <AppContent />
        <Toaster />
      </ConfirmDialogProvider>
    </ThemeProvider>
  );
}
