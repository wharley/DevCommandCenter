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
import { cn } from "@/lib/utils";
import {
	isProjectColorId,
	isProjectMascotId,
	isProjectSymbolId,
	PROJECT_COLOR_IDS,
	PROJECT_MASCOT_IDS,
	projectMascotPaths,
	resolveProjectColor,
	resolveProjectIcon,
	type ProjectColorId,
	type ProjectIconVisual,
	type ProjectMascotId,
	type ProjectSymbolId,
} from "@dcc/mascots";
import "./project-mascot.css";

export const PROJECT_ICON_OPTIONS = [
	{ id: "folder", Icon: Folder },
	{ id: "terminal", Icon: SquareTerminal },
	{ id: "code", Icon: Code2 },
	{ id: "layers", Icon: Layers3 },
	{ id: "package", Icon: Package },
	{ id: "database", Icon: Database },
	{ id: "globe", Icon: Globe2 },
	{ id: "rocket", Icon: Rocket },
	{ id: "branch", Icon: GitBranch },
	{ id: "cpu", Icon: Cpu },
	{ id: "shield", Icon: ShieldCheck },
	{ id: "wrench", Icon: Wrench },
] as const satisfies ReadonlyArray<{ id: ProjectSymbolId; Icon: LucideIcon }>;

export const PROJECT_COLOR_OPTIONS = PROJECT_COLOR_IDS;

export const PROJECT_MASCOT_OPTIONS = PROJECT_MASCOT_IDS;

export type ProjectIconId = ProjectSymbolId;
export type { ProjectColorId, ProjectIconVisual };

const PROJECT_COLOR_CLASSES: Record<ProjectColorId, string> = {
	slate: "border-slate-500/20 bg-slate-500/10 text-slate-600 dark:text-slate-300",
	sky: "border-sky-500/25 bg-sky-500/12 text-sky-600 dark:text-sky-300",
	cyan: "border-cyan-500/25 bg-cyan-500/12 text-cyan-600 dark:text-cyan-300",
	emerald:
		"border-emerald-500/25 bg-emerald-500/12 text-emerald-600 dark:text-emerald-300",
	amber: "border-amber-500/25 bg-amber-500/12 text-amber-700 dark:text-amber-300",
	orange: "border-orange-500/25 bg-orange-500/12 text-orange-600 dark:text-orange-300",
	rose: "border-rose-500/25 bg-rose-500/12 text-rose-600 dark:text-rose-300",
	violet:
		"border-violet-500/25 bg-violet-500/12 text-violet-600 dark:text-violet-300",
	indigo:
		"border-indigo-500/25 bg-indigo-500/12 text-indigo-600 dark:text-indigo-300",
	fuchsia:
		"border-fuchsia-500/25 bg-fuchsia-500/12 text-fuchsia-600 dark:text-fuchsia-300",
	lime: "border-lime-500/25 bg-lime-500/12 text-lime-600 dark:text-lime-300",
	pink: "border-pink-500/25 bg-pink-500/12 text-pink-600 dark:text-pink-300",
};

export function isKnownProjectIcon(value: string | null | undefined): value is string {
	return isProjectMascotId(value) || isProjectSymbolId(value);
}

export function isKnownProjectColor(value: string | null | undefined): value is ProjectColorId {
	return isProjectColorId(value);
}

export { resolveProjectIcon };

export function projectColorId(
	value: string | null | undefined,
	seed?: string | null,
): ProjectColorId {
	return resolveProjectColor(value, seed);
}

function ProjectLucideIcon({ id }: { id: ProjectIconId }) {
	const Icon = PROJECT_ICON_OPTIONS.find((option) => option.id === id)!.Icon;
	return <Icon strokeWidth={1.9} />;
}

function ProjectMascotSprite({ id, active }: { id: ProjectMascotId; active: boolean }) {
	const paths = projectMascotPaths(id);
	return (
		<svg
			viewBox="0 0 10 10"
			shapeRendering="crispEdges"
			fill="currentColor"
			style={{ width: "80%", height: "80%" }}
			className={cn("dcc-mascot", active && "is-active")}
		>
			<g className="dcc-mascot-rest">
				<path d={paths.rest.body} />
				{paths.rest.accent ? (
					<path className="dcc-mascot-accent" d={paths.rest.accent} />
				) : null}
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

export function ProjectIdentityGlyph({
	icon,
	color,
	seed,
	active = false,
	size = "md",
	className,
	title,
}: {
	icon?: string | null;
	color?: string | null;
	/** Stable project key (root path) — picks the auto mascot and color. */
	seed?: string | null;
	/** An agent is running in the project: the mascot animates. */
	active?: boolean;
	size?: "sm" | "md" | "lg";
	className?: string;
	title?: string;
}) {
	const visual = resolveProjectIcon(icon, seed);
	const resolvedColor = projectColorId(color, seed);

	return (
		<span
			aria-hidden
			title={title}
			className={cn(
				"grid shrink-0 place-items-center border",
				PROJECT_COLOR_CLASSES[resolvedColor],
				size === "sm" && "size-5 rounded-md [&_svg]:size-3",
				size === "md" && "size-7 rounded-lg [&_svg]:size-3.5",
				size === "lg" && "size-9 rounded-xl [&_svg]:size-4.5",
				className,
			)}
		>
			{visual.kind === "icon" ? (
				<ProjectLucideIcon id={visual.id} />
			) : (
				<ProjectMascotSprite id={visual.id} active={active} />
			)}
		</span>
	);
}
