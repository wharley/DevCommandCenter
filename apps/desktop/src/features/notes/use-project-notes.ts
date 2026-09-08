import { useEffect, useRef, useSyncExternalStore } from "react";
import { notesApi } from "./notes-api";
import { NotesStore } from "./notes-store";

export function useProjectNotes() {
	const ref = useRef<NotesStore | null>(null);
	if (!ref.current) {
		let storage: Storage | undefined;
		try {
			storage = window.localStorage;
		} catch {
			/* SQLite remains available. */
		}
		ref.current = new NotesStore(notesApi, storage);
	}
	const store = ref.current;
	const state = useSyncExternalStore(store.subscribe, store.getSnapshot);
	useEffect(() => {
		void store.load();
		const flush = () => {
			void store.flushAll().catch(() => {});
		};
		window.addEventListener("pagehide", flush);
		return () => {
			window.removeEventListener("pagehide", flush);
			flush();
		};
	}, [store]);
	return { store, ...state };
}
