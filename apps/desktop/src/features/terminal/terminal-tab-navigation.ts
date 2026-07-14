export function getTerminalTabNavigationTarget(
	tabIds: string[],
	currentId: string,
	key: string,
): string | null {
	if (tabIds.length === 0) return null;
	const currentIndex = Math.max(0, tabIds.indexOf(currentId));
	if (key === "Home") return tabIds[0] ?? null;
	if (key === "End") return tabIds[tabIds.length - 1] ?? null;
	if (key === "ArrowRight") return tabIds[(currentIndex + 1) % tabIds.length] ?? null;
	if (key === "ArrowLeft") {
		return tabIds[(currentIndex - 1 + tabIds.length) % tabIds.length] ?? null;
	}
	return null;
}
