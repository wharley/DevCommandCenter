import { lazy, useMemo } from "react";
import type { StreamdownProps } from "streamdown";
import "streamdown/styles.css";

export const LazyStreamdown = lazy(async () => {
	const [{ Streamdown }, { streamdownComponents }] = await Promise.all([
		import("streamdown"),
		import("@/components/streamdown-components"),
	]);

	const defaultShikiTheme: NonNullable<StreamdownProps["shikiTheme"]> = [
		"github-light",
		"github-dark",
	];
	const LazyRenderer = (props: StreamdownProps) => {
		const components = useMemo(
			() =>
				props.components
					? { ...streamdownComponents, ...props.components }
					: streamdownComponents,
			[props.components],
		);
		return (
			<Streamdown
				{...props}
				components={components}
				shikiTheme={props.shikiTheme ?? defaultShikiTheme}
			/>
		);
	};

	return {
		default: LazyRenderer,
	};
});
