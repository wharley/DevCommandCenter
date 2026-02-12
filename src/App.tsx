import { useEffect, useState } from "react";
import { Routes, Route } from "react-router-dom";
import { MainLayout } from "@/components/layouts/main-layout";
import { ThemeProvider } from "@/components/theme-provider";

// Pages
import HomePage from "@/src/pages/HomePage";
import SettingsPage from "@/src/pages/SettingsPage";
import ProjectPage from "@/src/pages/ProjectPage";
import MissionPage from "@/src/pages/MissionPage";
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
        <Route path="/project/:id" element={<ProjectPage />} />
        <Route
          path="/project/:id/mission/:missionId"
          element={<MissionPage />}
        />
      </Routes>
    </MainLayout>
  );
}

export default function App() {
  return <AppContent />;
}
