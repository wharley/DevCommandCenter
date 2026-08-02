import type { LucideIcon } from "lucide-react";
import {
	Code2,
	Database,
	Folder,
	Globe2,
	Layers3,
	Package,
	Rocket,
	SquareTerminal,
} from "lucide-react";
import { cn } from "@/lib/utils";

export const PROJECT_ICON_OPTIONS = [
	{ id: "folder", Icon: Folder },
	{ id: "terminal", Icon: SquareTerminal },
	{ id: "code", Icon: Code2 },
	{ id: "layers", Icon: Layers3 },
	{ id: "package", Icon: Package },
	{ id: "database", Icon: Database },
	{ id: "globe", Icon: Globe2 },
	{ id: "rocket", Icon: Rocket },
] as const satisfies ReadonlyArray<{ id: string; Icon: LucideIcon }>;

export const PROJECT_COLOR_OPTIONS = [
	"slate",
	"sky",
	"cyan",
	"emerald",
	"amber",
	"orange",
	"rose",
	"violet",
] as const;

export type ProjectIconId = (typeof PROJECT_ICON_OPTIONS)[number]["id"];
export type ProjectColorId = (typeof PROJECT_COLOR_OPTIONS)[number];

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
};

export function projectIconId(value: string | null | undefined): ProjectIconId {
	return PROJECT_ICON_OPTIONS.some((option) => option.id === value)
		? (value as ProjectIconId)
		: "folder";
}

export function projectColorId(value: string | null | undefined): ProjectColorId {
	return PROJECT_COLOR_OPTIONS.includes(value as ProjectColorId)
		? (value as ProjectColorId)
		: "slate";
}

export function ProjectIdentityGlyph({
	icon,
	color,
	size = "md",
	className,
	title,
}: {
	icon?: string | null;
	color?: string | null;
	size?: "sm" | "md" | "lg";
	className?: string;
	title?: string;
}) {
	const resolvedIcon = projectIconId(icon);
	const resolvedColor = projectColorId(color);
	const Icon = PROJECT_ICON_OPTIONS.find((option) => option.id === resolvedIcon)!.Icon;

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
			<Icon strokeWidth={1.9} />
		</span>
	);
}
