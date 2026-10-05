import type { LucideIcon } from "lucide-react";
import {
	Code2,
	Cpu,
	Database,
	Folder,
	GitBranch,
	Globe2,
	Layers3,
	Package,
	Rocket,
	ShieldCheck,
	SquareTerminal,
	Wrench,
} from "lucide-react";
import {
	projectMascotPaths,
	resolveProjectColor,
	resolveProjectIcon,
	type ProjectColorId,
	type ProjectSymbolId,
} from "@dcc/mascots";
import { cn } from "../lib/cn";

// The mobile companion is dark-only: the light shade of each project color.
const COLOR_CLASS: Record<ProjectColorId, string> = {
	slate: "text-slate-300",
	sky: "text-sky-300",
	cyan: "text-cyan-300",
	emerald: "text-emerald-300",
	amber: "text-amber-300",
	orange: "text-orange-300",
	rose: "text-rose-300",
	violet: "text-violet-300",
	indigo: "text-indigo-300",
	fuchsia: "text-fuchsia-300",
	lime: "text-lime-300",
	pink: "text-pink-300",
};

const SYMBOLS: Record<ProjectSymbolId, LucideIcon> = {
	folder: Folder,
	terminal: SquareTerminal,
	code: Code2,
	layers: Layers3,
	package: Package,
	database: Database,
	globe: Globe2,
	rocket: Rocket,
	branch: GitBranch,
	cpu: Cpu,
	shield: ShieldCheck,
	wrench: Wrench,
};

/**
 * The project's identity as the desktop shows it: the same Brazilian-fauna
 * mascot and color (both derived from the project path when unset), or the
 * symbol the person picked. The mascot animates while an agent runs.
 */
export function ProjectGlyph({
	icon,
	color,
	seed,
	active = false,
	className,
}: {
	icon?: string | null;
	color?: string | null;
	seed?: string | null;
	active?: boolean;
	className?: string;
}) {
	const visual = resolveProjectIcon(icon, seed);
	const tone = COLOR_CLASS[resolveProjectColor(color, seed)];
	if (visual.kind === "logo") {
		return (
			<img
				src={visual.src}
				alt=""
				draggable={false}
				className={cn("size-4 shrink-0 rounded-[3px] object-cover", className)}
			/>
		);
	}
	if (visual.kind === "icon") {
		const Icon = SYMBOLS[visual.id];
		return <Icon aria-hidden className={cn("size-3.5 shrink-0", tone, className)} />;
	}
	const paths = projectMascotPaths(visual.id);
	return (
		<svg
			aria-hidden
			viewBox="0 0 10 10"
			shapeRendering="crispEdges"
			fill="currentColor"
			className={cn("dcc-mascot size-4 shrink-0", active && "is-active", tone, className)}
		>
			<g className="dcc-mascot-rest">
				<path d={paths.rest.body} />
				{paths.rest.accent ? <path className="dcc-mascot-accent" d={paths.rest.accent} /> : null}
			</g>
			{active ? (
				<g className="dcc-mascot-move">
					<path d={paths.move.body} />
					{paths.move.accent ? (
						<path className="dcc-mascot-accent" d={paths.move.accent} />
					) : null}
				</g>
			) : null}
		</svg>
	);
}
