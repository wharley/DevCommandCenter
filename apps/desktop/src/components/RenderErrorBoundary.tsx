import { Component, type ErrorInfo, type ReactNode } from "react";
import i18n from "@/i18n/config";
import { recordFrontendError, type FrontendErrorContext } from "@/lib/frontend-diagnostics";
import { FrontendDiagnostics } from "./FrontendDiagnostics";

type Props = {
	children: ReactNode;
	scope: "app" | "workspace";
	resetKey?: string | null;
	context?: FrontendErrorContext;
};
type State = { failed: boolean; resetKey: Props["resetKey"] };

export class RenderErrorBoundary extends Component<Props, State> {
	state: State = { failed: false, resetKey: this.props.resetKey };

	static getDerivedStateFromError(): Partial<State> {
		return { failed: true };
	}

	static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
		return props.resetKey !== state.resetKey
			? { failed: false, resetKey: props.resetKey }
			: null;
	}

	componentDidCatch(error: Error, info: ErrorInfo) {
		recordFrontendError("react-caught", error, {
			scope: this.props.scope,
			componentStack: info.componentStack,
			context: this.props.context,
		});
	}

	render() {
		if (!this.state.failed) return this.props.children;
		return (
			<section role="alert" className="flex h-full min-h-64 w-full flex-1 items-center justify-center overflow-auto bg-background p-8 text-foreground">
				<div className="w-full max-w-lg space-y-4">
					<h1 className="text-lg font-semibold">{i18n.t("frontendRecovery.title")}</h1>
					<p className="text-sm text-muted-foreground">{i18n.t("frontendRecovery.body")}</p>
					<FrontendDiagnostics />
					<button type="button" className="rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground" onClick={() => window.location.reload()}>
						{i18n.t("frontendRecovery.reload")}
					</button>
				</div>
			</section>
		);
	}
}
