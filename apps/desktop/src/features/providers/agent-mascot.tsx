import { agentMascotPaths, type AgentMascotKey, type AgentMascotState } from "@dcc/mascots";
import { PixelSprite } from "@/components/pixel-sprite";
import { cn } from "@/lib/utils";
import { resolveIconKey } from "./provider-icons";

/**
 * Tile per provider: brand color where the brand has one (Claude clay, Codex
 * blue, Gemini violet); the monochrome marks stay neutral, like their logos,
 * and are told apart by the robot's antenna.
 */
const AGENT_TILE_CLASSES: Record<AgentMascotKey, string> = {
	claude: "border-[#D97757]/25 bg-[#D97757]/12 text-[#C2603F] dark:text-[#E8936F]",
	codex: "border-[#3B7BF6]/25 bg-[#3B7BF6]/12 text-[#2F6BE0] dark:text-[#7AA5FA]",
	gemini: "border-violet-500/25 bg-violet-500/12 text-violet-600 dark:text-violet-300",
	cursor: "border-border bg-muted/50 text-foreground/75",
	droid: "border-border bg-muted/50 text-foreground/75",
	grok: "border-border bg-muted/50 text-foreground/75",
	generic: "border-border bg-muted/50 text-foreground/60",
};

/**
 * Amber is "needs you" across DCC — it overrides the brand color. The solid
 * border keeps it apart from Claude's clay, which sits close to amber.
 */
const WAITING_TILE_CLASSES =
	"border-amber-500 bg-amber-500/15 text-amber-700 dark:border-amber-400 dark:text-amber-300";

/**
 * The agent's face in the timeline: a pixel robot facing its model label.
 * It walks while the agent works, waves when it needs you, and stands still
 * once the turn is over — finished messages never move.
 */
export function AgentMascot({
	provider,
	model,
	state = "idle",
	className,
	title,
}: {
	/** Provider id or label; preferred over `model` to pick the robot. */
	provider?: string | null;
	/** Model id or label, used when the provider is unknown. */
	model?: string | null;
	state?: AgentMascotState;
	className?: string;
	title?: string;
}) {
	const key: AgentMascotKey = resolveIconKey(provider) ?? resolveIconKey(model) ?? "generic";

	return (
		<span
			aria-hidden
			title={title}
			data-agent={key}
			data-state={state}
			className={cn(
				"grid size-6 shrink-0 place-items-center rounded-md border",
				state === "waiting" ? WAITING_TILE_CLASSES : AGENT_TILE_CLASSES[key],
				className,
			)}
		>
			<PixelSprite paths={agentMascotPaths(key, state)} active={state !== "idle"} size={20} />
		</span>
	);
}
