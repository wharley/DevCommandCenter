import { useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import {
  FolderGit2,
  Plus,
  Clock,
  GitBranch,
  ChevronRight,
  Search,
  MoreHorizontal,
  Trash2,
  ExternalLink,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Badge } from '@/components/ui/badge';
import { Empty } from '@/components/ui/empty';
import { AddProjectDialog } from '@/components/dialogs/add-project-dialog';
import { useProjects, useMissions } from '@/hooks/use-data';
import { formatDistanceToNow } from 'date-fns';
import { ptBR } from 'date-fns/locale';

export default function HomePage() {
  const [searchParams] = useSearchParams();
  const showNewDialog = searchParams.get('new') === 'true';
  
  const [search, setSearch] = useState('');
  const [dialogOpen, setDialogOpen] = useState(showNewDialog);

  const { projects, remove } = useProjects();
  const { missions } = useMissions();

  const filteredProjects = projects.filter((project) =>
    project.name.toLowerCase().includes(search.toLowerCase()) ||
    project.description?.toLowerCase().includes(search.toLowerCase())
  );

  const sortedProjects = [...filteredProjects].sort((a, b) => {
    const dateA = a.lastOpenedAt?.getTime() ?? a.createdAt.getTime();
    const dateB = b.lastOpenedAt?.getTime() ?? b.createdAt.getTime();
    return dateB - dateA;
  });

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="flex items-center justify-between border-b border-border bg-card px-6 py-4">
        <div>
          <h1 className="text-xl font-semibold text-card-foreground">Projetos</h1>
          <p className="text-sm text-muted-foreground">
            Gerencie seus repositórios e missões de código
          </p>
        </div>
        <Button onClick={() => setDialogOpen(true)}>
          <Plus className="mr-2 h-4 w-4" />
          Adicionar projeto
        </Button>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-auto p-6">
        {/* Search */}
        <div className="mb-6 flex items-center gap-4">
          <div className="relative flex-1 max-w-md">
            <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder="Buscar projetos..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="pl-10"
            />
          </div>
          <Badge variant="secondary" className="text-muted-foreground">
            {filteredProjects.length} projeto{filteredProjects.length !== 1 ? 's' : ''}
          </Badge>
        </div>

        {/* Projects Grid */}
        {sortedProjects.length === 0 ? (
          <Empty className="mt-20">
            <Empty.Icon>
              <FolderGit2 className="h-10 w-10" />
            </Empty.Icon>
            <Empty.Title>Nenhum projeto ainda</Empty.Title>
            <Empty.Description>
              {search
                ? 'Nenhum projeto corresponde à busca. Tente outro termo.'
                : 'Adicione seu primeiro projeto para criar missões de código com IA.'}
            </Empty.Description>
            {!search && (
              <Empty.Actions>
                <Button onClick={() => setDialogOpen(true)}>
                  <Plus className="mr-2 h-4 w-4" />
                  Adicionar projeto
                </Button>
              </Empty.Actions>
            )}
          </Empty>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {sortedProjects.map((project) => {
              const projectMissions = missions.filter((m) => m.projectId === project.id);
              const activeMissions = projectMissions.filter(
                (m) => !['completed', 'failed', 'cancelled'].includes(m.status)
              ).length;

              return (
                <Card
                  key={project.id}
                  className="group relative transition-shadow hover:shadow-md"
                >
                  <Link to={`/project/${project.id}`} className="absolute inset-0 z-10" />
                  
                  <CardHeader className="pb-3">
                    <div className="flex items-start justify-between">
                      <div className="flex items-center gap-3">
                        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
                          <FolderGit2 className="h-5 w-5 text-primary" />
                        </div>
                        <div>
                          <CardTitle className="text-base">{project.name}</CardTitle>
                          {project.gitRemoteUrl && (
                            <div className="flex items-center gap-1 text-xs text-muted-foreground">
                              <GitBranch className="h-3 w-3" />
                              <span className="truncate max-w-[150px]">main</span>
                            </div>
                          )}
                        </div>
                      </div>
                      
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="relative z-20 h-8 w-8 opacity-0 transition-opacity group-hover:opacity-100"
                          >
                            <MoreHorizontal className="h-4 w-4" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem asChild>
                            <Link to={`/project/${project.id}`}>
                              <ExternalLink className="mr-2 h-4 w-4" />
                              Abrir projeto
                            </Link>
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem
                            className="text-destructive focus:text-destructive"
                            onClick={(e) => {
                              e.preventDefault();
                              remove(project.id);
                            }}
                          >
                            <Trash2 className="mr-2 h-4 w-4" />
                            Excluir
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </div>
                  </CardHeader>
                  
                  <CardContent>
                    {project.description && (
                      <CardDescription className="mb-4 line-clamp-2">
                        {project.description}
                      </CardDescription>
                    )}
                    
                    <div className="flex items-center justify-between text-xs text-muted-foreground">
                      <div className="flex items-center gap-4">
                        {activeMissions > 0 && (
                          <Badge variant="secondary" className="text-xs">
                            {activeMissions} ativo{activeMissions !== 1 ? 's' : ''}
                          </Badge>
                        )}
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {formatDistanceToNow(
                            project.lastOpenedAt ?? project.createdAt,
                            { addSuffix: true, locale: ptBR }
                          )}
                        </span>
                      </div>
                      <ChevronRight className="h-4 w-4 transition-transform group-hover:translate-x-1" />
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>
        )}
      </div>

      {/* Add Project Dialog */}
      <AddProjectDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </div>
  );
}
