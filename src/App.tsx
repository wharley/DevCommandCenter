import { Routes, Route } from "react-router-dom";
import { MainLayout } from "@/components/layouts/main-layout";

// Pages
import HomePage from "@/src/pages/HomePage";
import SettingsPage from "@/src/pages/SettingsPage";
import ProjectPage from "@/src/pages/ProjectPage";
import MissionPage from "@/src/pages/MissionPage";

export default function App() {
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
