import { Link, useLocation } from "react-router-dom";
import {
  FolderGit2,
  Rocket,
  Settings,
  Terminal,
  Plus,
  ChevronRight,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { useProjects } from "@/hooks/use-data";
import { useAppStore } from "@/hooks/use-app-store";

const navItems = [
  { href: "/", label: "Projetos", icon: FolderGit2 },
  { href: "/settings", label: "Configurações", icon: Settings },
];

export function AppSidebar() {
  const { pathname } = useLocation();
  const { projects } = useProjects();
  const currentProjectId = useAppStore((s) => s.currentProjectId);
  const recentProjects = projects
    .sort((a, b) => {
      const dateA = a.lastOpenedAt?.getTime() ?? 0;
      const dateB = b.lastOpenedAt?.getTime() ?? 0;
      return dateB - dateA;
    })
    .slice(0, 5);

  return (
    <aside className="flex h-screen w-64 min-h-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
      {/* Logo */}
      <div className="flex h-14 items-center gap-2 border-b border-sidebar-border px-4 mt-8">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-sidebar-primary">
          <Terminal className="h-4 w-4 text-sidebar-primary-foreground" />
        </div>
        <span className="font-semibold tracking-tight">Dev Command</span>
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
                  <span className="truncate">{project.name}</span>
                  <ChevronRight className="ml-auto h-4 w-4 opacity-0 transition-opacity group-hover:opacity-100" />
                </Link>
              );
            })
          )}
        </div>
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
    </aside>
  );
}
