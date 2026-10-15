/** The project the home is scoped to, remembered on this phone (null: every project). */
import { deleteItem, getItem, setItem } from "./secureStorage";

const KEY = "dcc.home.scope";

export function loadScope(): Promise<string | null> {
	return getItem(KEY).catch(() => null);
}

export function saveScope(projectKey: string | null): void {
	void (projectKey ? setItem(KEY, projectKey) : deleteItem(KEY)).catch(() => {});
}
