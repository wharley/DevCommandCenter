/** Which page the home opens on, remembered on this phone: the rooms first. */
import { getItem, setItem } from "../backend/secureStorage";

export type HomePage = "rooms" | "tasks";

const KEY = "dcc.home.page";

export function loadHomePage(): Promise<HomePage> {
	return getItem(KEY)
		.catch(() => null)
		.then((value) => (value === "tasks" ? "tasks" : "rooms"));
}

export function saveHomePage(page: HomePage): void {
	void setItem(KEY, page).catch(() => {});
}
