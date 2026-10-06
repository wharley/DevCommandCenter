import type { MascotPaths } from "@dcc/mascots";
import { cn } from "@/lib/utils";
import "./pixel-sprite.css";

/**
 * Renders a two-frame pixel sprite from `@dcc/mascots` (project fauna, agent
 * robots). While `active`, the `rest` and `move` frames alternate.
 */
export function PixelSprite({
	paths,
	active,
	size,
}: {
	paths: MascotPaths;
	active: boolean;
	/** Exact size in px — a multiple of 10 keeps every pixel crisp. Default: 80% of the tile. */
	size?: number;
}) {
	return (
		<svg
			viewBox="0 0 10 10"
			shapeRendering="crispEdges"
			fill="currentColor"
			style={size ? { width: size, height: size } : { width: "80%", height: "80%" }}
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
