import { useEffect, useState } from "react";
import { Routes, Route } from "react-router-dom";
import { MainLayout } from "@/components/layouts/main-layout";
import { ThemeProvider } from "@/components/theme-provider";

// Pages
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
      <ThemeProvider
        attribute="class"
        defaultTheme="dark"
        enableSystem
        disableTransitionOnChange
      >
        <div className="flex min-h-screen items-center justify-center bg-background">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
        </div>
      </ThemeProvider>
    );
  }

  if (isElectron && !isActivated) {
    return (
      <ThemeProvider
        attribute="class"
        defaultTheme="dark"
        enableSystem
        disableTransitionOnChange
      >
        <ActivationPage onActivated={() => setIsActivated(true)} />
      </ThemeProvider>
    );
  }

  return (
    <MainLayout>
      <Routes>
        <Route path="/" element={<HomePage />} />
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
      </Routes>
    </MainLayout>
  );
}

export default function App() {
  return <AppContent />;
}
