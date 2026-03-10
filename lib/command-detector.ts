/**
 * Command Detector - Detecção de comandos pendentes em texto
 *
 * Detecta comandos de terminal (npm, yarn, pip, etc.) que precisam
 * ser executados manualmente pelo usuário.
 */

import type { PendingCommand } from "./database/types";

interface CommandPattern {
  regex: RegExp;
  type: string;
  description?: string;
}

const COMMAND_PATTERNS: CommandPattern[] = [
  // Node.js / JavaScript
  {
    regex: /\b(npm\s+install(?:\s+[\w@\-\/\.]+)*)/gi,
    type: "npm",
    description: "Instalar dependências npm",
  },
  {
    regex: /\b(npm\s+i(?:\s+[\w@\-\/\.]+)+)/gi,
    type: "npm",
    description: "Instalar dependências npm",
  },
  {
    regex: /\b(yarn\s+add(?:\s+[\w@\-\/\.]+)+)/gi,
    type: "yarn",
    description: "Instalar dependências yarn",
  },
  {
    regex: /\b(yarn\s+install)\b/gi,
    type: "yarn",
    description: "Instalar dependências yarn",
  },
  {
    regex: /\b(yarn)\s*$/gim,
    type: "yarn",
    description: "Instalar dependências yarn",
  },
  {
    regex: /\b(pnpm\s+(?:add|install)(?:\s+[\w@\-\/\.]+)*)/gi,
    type: "pnpm",
    description: "Instalar dependências pnpm",
  },
  {
    regex: /\b(bun\s+(?:add|install)(?:\s+[\w@\-\/\.]+)*)/gi,
    type: "bun",
    description: "Instalar dependências bun",
  },

  // NPX commands
  {
    regex: /\b(npx\s+prisma\s+(?:migrate\s+dev|migrate\s+deploy|generate|db\s+push|db\s+pull|studio)[\w\s\-]*)/gi,
    type: "prisma",
    description: "Executar comando Prisma",
  },
  {
    regex: /\b(npx\s+drizzle-kit\s+[\w\s\-:]+)/gi,
    type: "drizzle",
    description: "Executar comando Drizzle",
  },
  {
    regex: /\b(npx\s+typeorm\s+[\w\s\-:]+)/gi,
    type: "typeorm",
    description: "Executar comando TypeORM",
  },
  {
    regex: /\b(npx\s+create-[\w\-]+(?:\s+[\w\-\.\/]+)*)/gi,
    type: "npx",
    description: "Criar projeto com npx",
  },
  {
    regex: /\b(npx\s+[\w@\-\/\.]+(?:\s+[\w\-]+)*)/gi,
    type: "npx",
    description: "Executar comando npx",
  },

  // Python
  {
    regex: /\b(pip\s+install(?:\s+[\w\-\[\]\.\>=<]+)*)/gi,
    type: "pip",
    description: "Instalar dependências pip",
  },
  {
    regex: /\b(pip3\s+install(?:\s+[\w\-\[\]\.\>=<]+)*)/gi,
    type: "pip",
    description: "Instalar dependências pip3",
  },
  {
    regex: /\b(poetry\s+(?:add|install)(?:\s+[\w\-\.]+)*)/gi,
    type: "poetry",
    description: "Gerenciar dependências Poetry",
  },
  {
    regex: /\b(pipenv\s+install(?:\s+[\w\-\.]+)*)/gi,
    type: "pipenv",
    description: "Instalar dependências Pipenv",
  },
  {
    regex: /\b(python\s+-m\s+pip\s+install(?:\s+[\w\-\[\]\.\>=<]+)*)/gi,
    type: "pip",
    description: "Instalar dependências pip",
  },

  // Ruby
  {
    regex: /\b(bundle\s+install)\b/gi,
    type: "bundle",
    description: "Instalar gems com Bundler",
  },
  {
    regex: /\b(gem\s+install(?:\s+[\w\-]+)*)/gi,
    type: "gem",
    description: "Instalar gem Ruby",
  },

  // Go
  {
    regex: /\b(go\s+get(?:\s+[\w\-\/\.@]+)*)/gi,
    type: "go",
    description: "Baixar dependências Go",
  },
  {
    regex: /\b(go\s+mod\s+tidy)\b/gi,
    type: "go",
    description: "Limpar dependências Go",
  },
  {
    regex: /\b(go\s+mod\s+download)\b/gi,
    type: "go",
    description: "Baixar dependências Go",
  },

  // Rust
  {
    regex: /\b(cargo\s+(?:add|install)(?:\s+[\w\-]+)*)/gi,
    type: "cargo",
    description: "Instalar crate Rust",
  },
  {
    regex: /\b(cargo\s+build(?:\s+[\w\-]+)*)/gi,
    type: "cargo",
    description: "Compilar projeto Rust",
  },

  // Docker
  {
    regex: /\b(docker\s+build(?:\s+[\w\s\-\.\/=:]+)*)/gi,
    type: "docker",
    description: "Build de imagem Docker",
  },
  {
    regex: /\b(docker\s+compose\s+(?:up|build|pull)(?:\s+[\w\s\-]+)*)/gi,
    type: "docker",
    description: "Docker Compose",
  },
  {
    regex: /\b(docker-compose\s+(?:up|build|pull)(?:\s+[\w\s\-]+)*)/gi,
    type: "docker",
    description: "Docker Compose",
  },

  // PHP
  {
    regex: /\b(composer\s+(?:install|require|update)(?:\s+[\w\-\/]+)*)/gi,
    type: "composer",
    description: "Gerenciar dependências Composer",
  },

  // .NET
  {
    regex: /\b(dotnet\s+(?:restore|add\s+package)(?:\s+[\w\-\.]+)*)/gi,
    type: "dotnet",
    description: "Gerenciar dependências .NET",
  },

  // Java/Maven/Gradle
  {
    regex: /\b(mvn\s+(?:install|clean\s+install|dependency:resolve)[\w\s\-]*)/gi,
    type: "maven",
    description: "Build Maven",
  },
  {
    regex: /\b(gradle\s+(?:build|dependencies)[\w\s\-]*)/gi,
    type: "gradle",
    description: "Build Gradle",
  },

  // Database migrations genéricos
  {
    regex: /\b(rails\s+db:migrate)\b/gi,
    type: "rails",
    description: "Migration Rails",
  },
  {
    regex: /\b(flask\s+db\s+(?:upgrade|migrate)[\w\s\-]*)/gi,
    type: "flask",
    description: "Migration Flask",
  },
  {
    regex: /\b(alembic\s+upgrade[\w\s\-]*)/gi,
    type: "alembic",
    description: "Migration Alembic",
  },
];

let commandIdCounter = 0;

function generateCommandId(): string {
  commandIdCounter++;
  return `cmd-${Date.now()}-${commandIdCounter}`;
}

function normalizeCommand(command: string): string {
  return command
    .replace(/\s+/g, " ")
    .trim()
    .replace(/^[$>]\s*/, "");
}

/**
 * Extrai comandos pendentes de um texto
 */
export function extractPendingCommands(
  text: string,
  source: PendingCommand["source"] = "plan"
): PendingCommand[] {
  if (!text || typeof text !== "string") {
    return [];
  }

  const commands: PendingCommand[] = [];
  const seenCommands = new Set<string>();

  for (const pattern of COMMAND_PATTERNS) {
    const regex = new RegExp(pattern.regex.source, pattern.regex.flags);
    let match: RegExpExecArray | null;

    while ((match = regex.exec(text)) !== null) {
      const rawCommand = match[1] || match[0];
      const normalized = normalizeCommand(rawCommand);

      if (normalized && !seenCommands.has(normalized.toLowerCase())) {
        seenCommands.add(normalized.toLowerCase());
        commands.push({
          id: generateCommandId(),
          command: normalized,
          description: pattern.description,
          source,
          confirmedAt: null,
        });
      }
    }
  }

  return commands;
}

/**
 * Extrai comandos de um MissionPlan (summary + steps)
 */
export function extractCommandsFromPlan(plan: {
  summary?: string;
  steps?: Array<{ description?: string; title?: string }>;
}): PendingCommand[] {
  const commands: PendingCommand[] = [];
  const seenCommands = new Set<string>();

  if (plan.summary) {
    for (const cmd of extractPendingCommands(plan.summary, "plan")) {
      if (!seenCommands.has(cmd.command.toLowerCase())) {
        seenCommands.add(cmd.command.toLowerCase());
        commands.push(cmd);
      }
    }
  }

  if (plan.steps) {
    for (const step of plan.steps) {
      const text = `${step.title || ""} ${step.description || ""}`;
      for (const cmd of extractPendingCommands(text, "plan")) {
        if (!seenCommands.has(cmd.command.toLowerCase())) {
          seenCommands.add(cmd.command.toLowerCase());
          commands.push(cmd);
        }
      }
    }
  }

  return commands;
}

/**
 * Extrai comandos de GeneratedCode (summary + arquivos de config como package.json)
 */
export function extractCommandsFromCode(code: {
  summary?: string;
  files?: Array<{ path?: string; suggestedContent?: string; action?: string }>;
}): PendingCommand[] {
  const commands: PendingCommand[] = [];
  const seenCommands = new Set<string>();

  if (code.summary) {
    for (const cmd of extractPendingCommands(code.summary, "code")) {
      if (!seenCommands.has(cmd.command.toLowerCase())) {
        seenCommands.add(cmd.command.toLowerCase());
        commands.push(cmd);
      }
    }
  }

  if (code.files) {
    for (const file of code.files) {
      if (!file.path) continue;

      const isPackageJson =
        file.path.endsWith("package.json") ||
        file.path.endsWith("requirements.txt") ||
        file.path.endsWith("Gemfile") ||
        file.path.endsWith("Cargo.toml") ||
        file.path.endsWith("go.mod") ||
        file.path.endsWith("composer.json") ||
        file.path.endsWith("pyproject.toml");

      if (
        isPackageJson &&
        (file.action === "create" || file.action === "modify")
      ) {
        const installCommands = getInstallCommandForFile(file.path);
        for (const cmd of installCommands) {
          if (!seenCommands.has(cmd.command.toLowerCase())) {
            seenCommands.add(cmd.command.toLowerCase());
            commands.push(cmd);
          }
        }
      }

      if (file.suggestedContent) {
        for (const cmd of extractPendingCommands(
          file.suggestedContent,
          "file"
        )) {
          if (!seenCommands.has(cmd.command.toLowerCase())) {
            seenCommands.add(cmd.command.toLowerCase());
            commands.push(cmd);
          }
        }
      }
    }
  }

  return commands;
}

/**
 * Retorna o comando de instalação apropriado para um arquivo de dependências
 */
function getInstallCommandForFile(filePath: string): PendingCommand[] {
  const fileName = filePath.split("/").pop() || filePath;

  const installCommands: Record<string, PendingCommand> = {
    "package.json": {
      id: generateCommandId(),
      command: "npm install",
      description: "Instalar dependências do package.json",
      source: "file",
      confirmedAt: null,
    },
    "requirements.txt": {
      id: generateCommandId(),
      command: "pip install -r requirements.txt",
      description: "Instalar dependências Python",
      source: "file",
      confirmedAt: null,
    },
    Gemfile: {
      id: generateCommandId(),
      command: "bundle install",
      description: "Instalar gems Ruby",
      source: "file",
      confirmedAt: null,
    },
    "Cargo.toml": {
      id: generateCommandId(),
      command: "cargo build",
      description: "Compilar projeto Rust",
      source: "file",
      confirmedAt: null,
    },
    "go.mod": {
      id: generateCommandId(),
      command: "go mod tidy",
      description: "Atualizar dependências Go",
      source: "file",
      confirmedAt: null,
    },
    "composer.json": {
      id: generateCommandId(),
      command: "composer install",
      description: "Instalar dependências PHP",
      source: "file",
      confirmedAt: null,
    },
    "pyproject.toml": {
      id: generateCommandId(),
      command: "poetry install",
      description: "Instalar dependências Poetry",
      source: "file",
      confirmedAt: null,
    },
  };

  const cmd = installCommands[fileName];
  return cmd ? [cmd] : [];
}

/**
 * Combina comandos de plan e code, removendo duplicatas
 */
export function mergeCommands(
  ...commandArrays: PendingCommand[][]
): PendingCommand[] {
  const seenCommands = new Set<string>();
  const result: PendingCommand[] = [];

  for (const commands of commandArrays) {
    for (const cmd of commands) {
      const key = cmd.command.toLowerCase();
      if (!seenCommands.has(key)) {
        seenCommands.add(key);
        result.push(cmd);
      }
    }
  }

  return result;
}

/**
 * Verifica se há comandos não confirmados
 */
export function hasUnconfirmedCommands(
  commands: PendingCommand[] | null | undefined
): boolean {
  if (!commands || commands.length === 0) return false;
  return commands.some((cmd) => !cmd.confirmedAt);
}

/**
 * Retorna apenas os comandos não confirmados
 */
export function getUnconfirmedCommands(
  commands: PendingCommand[] | null | undefined
): PendingCommand[] {
  if (!commands) return [];
  return commands.filter((cmd) => !cmd.confirmedAt);
}
