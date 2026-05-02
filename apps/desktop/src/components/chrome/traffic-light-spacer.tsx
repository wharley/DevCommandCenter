export function TrafficLightSpacer({
	side,
	width,
}: {
	side: "left" | "right";
	width: number;
}) {
	return (
		<div
			aria-hidden
			className="shrink-0"
			style={{
				width,
				marginLeft: side === "left" ? 0 : "auto",
				marginRight: side === "right" ? 0 : "auto",
			}}
		/>
	);
}
