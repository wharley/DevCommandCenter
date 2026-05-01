import { lazy } from "react";
import type { StreamdownProps } from "streamdown";
import "streamdown/styles.css";

export const LazyStreamdown = lazy(async () => {
	const [{ Streamdown }, { streamdownComponents }] = await Promise.all([
		import("streamdown"),
		import("@/components/streamdown-components"),
	]);

	const LazyRenderer = (props: StreamdownProps) => (
		<Streamdown
			{...props}
			components={{ ...streamdownComponents, ...props.components }}
			shikiTheme={props.shikiTheme ?? ["github-light", "github-dark"]}
		/>
	);

	return {
		default: LazyRenderer,
	};
});
