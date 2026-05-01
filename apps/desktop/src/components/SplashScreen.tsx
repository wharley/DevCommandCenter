import { useSplashScreen } from "@/hooks/use-splash-screen";

export function SplashScreen() {
	const { isHiding, isVisible } = useSplashScreen();

	if (!isVisible) {
		return null;
	}

	return (
		<div
			aria-hidden="true"
			className={[
				"pointer-events-none fixed inset-0 z-[80] flex items-center justify-center bg-background",
				"transition-opacity duration-700 ease-out",
				isHiding ? "opacity-0" : "opacity-100",
			].join(" ")}
		>
			<div className="flex flex-col items-center gap-3 text-center">
				<div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-card shadow-sm">
					<div className="h-5 w-5 rounded-full bg-foreground/90" />
				</div>
				<div className="space-y-1">
					<p className="text-[13px] font-medium tracking-[-0.01em] text-foreground">
						Dev Command Center
					</p>
					<p className="text-[11px] text-muted-foreground">
						Initializing workspace shell
					</p>
				</div>
			</div>
		</div>
	);
}
