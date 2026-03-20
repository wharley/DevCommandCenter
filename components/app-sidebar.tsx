import { useState } from "react";
import { Link, useLocation } from "react-router-dom";
import {
  FolderGit2,
  Rocket,
  Settings,
  Terminal,
  Plus,
  ChevronRight,
  ChevronLeft,
  HelpCircle,
  PanelLeftOpen,
} from "lucide-react";
import { HelpDialog } from "@/components/dialogs/help-dialog";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { useProjects, useMissions } from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";

const navItems = [
  { href: "/", label: "Hive", icon: Terminal },
  { href: "/projects", label: "Projetos", icon: FolderGit2 },
  { href: "/settings", label: "Configurações", icon: Settings },
];

/** Gera uma cor estável por projeto (HSL) para uso em badges/círculos. */
function projectAccentColor(projectId: string): { bg: string; fg: string } {
  let hash = 0;
  for (let i = 0; i < projectId.length; i++) {
    hash = projectId.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash % 360);
  const sat = 52;
  const light = 42;
  return {
    bg: `hsl(${hue}, ${sat}%, ${light}%)`,
    fg: "hsl(0, 0%, 100%)",
  };
}

export function AppSidebar() {
  const { pathname } = useLocation();
  const [helpOpen, setHelpOpen] = useState(false);
  const { projects } = useProjects();
  const { missions } = useMissions();
  const currentProjectId = useAppStore((s) => s.currentProjectId);
  const sidebarCollapsed = useAppStore((s) => s.sidebarCollapsed);
  const setSidebarCollapsed = useAppStore((s) => s.setSidebarCollapsed);
  const recentProjects = projects
    .sort((a, b) => {
      const dateA =
        a.lastOpenedAt?.getTime() ?? a.createdAt.getTime();
      const dateB =
        b.lastOpenedAt?.getTime() ?? b.createdAt.getTime();
      return dateB - dateA;
    })
    .slice(0, 5);

  if (sidebarCollapsed) {
    return (
      <aside className="flex h-screen w-14 min-h-0 flex-col items-center border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-all duration-200">
        <div className="flex h-14 items-center justify-center border-b border-sidebar-border mt-8 w-full">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-sidebar-primary">
            <Terminal className="h-4 w-4 text-sidebar-primary-foreground" />
          </div>
        </div>

        <nav className="flex flex-col items-center gap-1 p-2 w-full">
          {navItems.map((item) => {
            const isActive = pathname === item.href;
            return (
              <Link
                key={item.href}
                to={item.href}
                title={item.label}
                className={cn(
                  "electron-no-drag flex h-9 w-9 items-center justify-center rounded-lg transition-colors",
                  isActive
                    ? "bg-sidebar-accent text-sidebar-accent-foreground"
                    : "text-sidebar-foreground/70 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground",
                )}
              >
                <item.icon className="h-4 w-4" />
              </Link>
            );
          })}
        </nav>

        <Separator className="my-2 bg-sidebar-border" />

        <div className="flex-1 min-h-0 overflow-y-auto w-full flex flex-col items-center gap-1 px-1 py-2">
          {recentProjects.map((project) => {
            const isActive = currentProjectId === project.id;
            const accent = projectAccentColor(project.id);
            return (
              <Link
                key={project.id}
                to={`/project/${project.id}`}
                title={project.name}
                className={cn(
                  "electron-no-drag flex h-9 w-9 items-center justify-center rounded-lg transition-colors",
                  isActive
                    ? "bg-sidebar-accent text-sidebar-accent-foreground"
                    : "text-sidebar-foreground/70 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground",
                )}
              >
                <span
                  className="flex h-7 w-7 items-center justify-center rounded-full text-[11px] font-semibold"
                  style={{ backgroundColor: accent.bg, color: accent.fg }}
                >
                  {project.name.charAt(0).toUpperCase()}
                </span>
              </Link>
            );
          })}
        </div>

        <div className="border-t border-sidebar-border p-2 w-full flex justify-center">
          <Button
            variant="ghost"
            size="icon"
            className="electron-no-drag h-9 w-9 text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={() => setSidebarCollapsed(false)}
            title="Expandir sidebar"
          >
            <PanelLeftOpen className="h-4 w-4" />
          </Button>
        </div>

        <HelpDialog open={helpOpen} onOpenChange={setHelpOpen} />
      </aside>
    );
  }

  return (
    <aside className="flex h-screen w-64 min-h-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-all duration-200">
      {/* Logo */}
      <div className="flex h-14 items-center gap-2 border-b border-sidebar-border px-4 mt-8">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-sidebar-primary">
          <Terminal className="h-4 w-4 text-sidebar-primary-foreground" />
        </div>
        <span className="font-semibold tracking-tight">Dev Command</span>
        <Button
          variant="ghost"
          size="icon"
          className="electron-no-drag ml-auto h-7 w-7 text-sidebar-foreground/50 hover:bg-sidebar-accent hover:text-sidebar-foreground"
          onClick={() => setSidebarCollapsed(true)}
          title="Colapsar sidebar"
        >
          <ChevronLeft className="h-4 w-4" />
        </Button>
      </div>

      {/* Navigation - no Tooltip to avoid ref/setState loop with Link + asChild */}
      <nav className="flex flex-col gap-1 p-2">
        {navItems.map((item) => {
          const isActive = pathname === item.href;
          return (
            <Link
              key={item.href}
              to={item.href}
              className={cn(
                "electron-no-drag flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
                isActive
                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                  : "text-sidebar-foreground/70 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground",
              )}
            >
              <item.icon className="h-4 w-4" />
              {item.label}
            </Link>
          );
        })}
      </nav>

      <Separator className="my-2 bg-sidebar-border" />

      {/* Projetos recentes */}
      <div className="flex items-center justify-between px-4 py-2">
        <span className="text-xs font-medium uppercase tracking-wider text-sidebar-foreground/50">
          Projetos recentes
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="electron-no-drag h-6 w-6 text-sidebar-foreground/50 hover:bg-sidebar-accent hover:text-sidebar-foreground"
          asChild
        >
          <Link
            to="/?new=true"
            className="electron-no-drag"
            title="Adicionar projeto"
          >
            <Plus className="h-3.5 w-3.5" />
          </Link>
        </Button>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-2">
        <div className="flex flex-col gap-1 py-2">
          {recentProjects.length === 0 ? (
            <p className="px-3 py-2 text-xs text-sidebar-foreground/50">
              Nenhum projeto ainda
            </p>
          ) : (
            recentProjects.map((project) => {
              const isActive = currentProjectId === project.id;
              const missionCount = missions.filter(
                (m) => m.projectId === project.id
              ).length;
              const accent = projectAccentColor(project.id);
              return (
                <Link
                  key={project.id}
                  to={`/project/${project.id}`}
                  className={cn(
                    "electron-no-drag group flex items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors",
                    isActive
                      ? "bg-sidebar-accent text-sidebar-accent-foreground"
                      : "text-sidebar-foreground/70 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground",
                  )}
                >
                  <FolderGit2 className="h-4 w-4 shrink-0" />
                  <span className="truncate flex-1 min-w-0">
                    {project.name}
                  </span>
                  {missionCount > 0 && (
                    <span
                      className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold tabular-nums shadow-sm ring-2 ring-sidebar/80"
                      style={{
                        backgroundColor: accent.bg,
                        color: accent.fg,
                      }}
                      title={`${missionCount} missão${missionCount !== 1 ? "ões" : ""}`}
                    >
                      {missionCount > 99 ? "99+" : missionCount}
                    </span>
                  )}
                  <ChevronRight className="h-4 w-4 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
                </Link>
              );
            })
          )}
        </div>
      </div>

      {/* Ajuda */}
      <div className="border-t border-sidebar-border p-2">
        <Button
          variant="ghost"
          size="sm"
          className="electron-no-drag w-full justify-start gap-3 text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
          onClick={() => setHelpOpen(true)}
          title="Ajuda e atalhos (⌘/)"
        >
          <HelpCircle className="h-4 w-4" />
          Ajuda
        </Button>
      </div>

      {/* Footer */}
      <div className="border-t border-sidebar-border p-4">
        <div className="flex items-center gap-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-full bg-sidebar-accent">
            <Rocket className="h-4 w-4 text-sidebar-primary" />
          </div>
          <div className="flex-1 truncate">
            <p className="text-xs font-medium">Dev Command Center</p>
            <p className="text-xs text-sidebar-foreground/50">v0.1.0</p>
          </div>
        </div>
      </div>

      <HelpDialog open={helpOpen} onOpenChange={setHelpOpen} />
    </aside>
  );
}
