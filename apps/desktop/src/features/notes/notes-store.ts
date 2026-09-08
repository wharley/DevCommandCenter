import type { CreateNote, NotePatch, ProjectNote } from "./notes-api";

type NotesApi = {
	list: () => Promise<ProjectNote[]>;
	create: (input: CreateNote) => Promise<ProjectNote>;
	update: (note: ProjectNote) => Promise<ProjectNote>;
	delete: (ids: string[]) => Promise<void>;
};
export type NotesSnapshot = {
	notes: ProjectNote[];
	loading: boolean;
	error: string | null;
	saving: string[];
	failed: string[];
};
const DRAFT_KEY = "dcc.notes.pending.v1";

/** Serializes writes per note. A delayed response never replaces newer typing. */
export class NotesStore {
	private state: NotesSnapshot = {
		notes: [],
		loading: true,
		error: null,
		saving: [],
		failed: [],
	};
	private base = new Map<string, ProjectNote>();
	private pending = new Map<string, NotePatch>();
	private running = new Map<string, Promise<void>>();
	private timers = new Map<string, ReturnType<typeof setTimeout>>();
	private listeners = new Set<() => void>();
	private deleting = new Set<string>();
	private membershipVersion = 0;
	private loadSequence = 0;
	constructor(
		private api: NotesApi,
		private storage?: Pick<Storage, "getItem" | "setItem">,
	) {}
	getSnapshot = () => this.state;
	subscribe = (listener: () => void) => {
		this.listeners.add(listener);
		return () => {
			this.listeners.delete(listener);
		};
	};
	private emit(patch: Partial<NotesSnapshot> = {}) {
		this.state = { ...this.state, ...patch };
		for (const listener of this.listeners) listener();
	}
	private backup() {
		const dirty = new Set([...this.pending.keys(), ...this.running.keys()]);
		const drafts = this.state.notes
			.filter((note) => dirty.has(note.id))
			.map((note) => [note.id, { title: note.title, content: note.content }]);
		try {
			this.storage?.setItem(
				DRAFT_KEY,
				JSON.stringify(Object.fromEntries(drafts)),
			);
		} catch {
			/* Database remains the primary store. */
		}
	}
	async load() {
		const membershipVersion = this.membershipVersion;
		const sequence = ++this.loadSequence;
		try {
			const notes = await this.api.list();
			if (
				membershipVersion !== this.membershipVersion ||
				sequence !== this.loadSequence
			)
				return;
			if (this.state.loading) {
				try {
					const drafts = JSON.parse(this.storage?.getItem(DRAFT_KEY) ?? "{}");
					for (const note of notes) {
						const draft = drafts[note.id];
						// Recover text only; never replay task creation, completion or deletion.
						if (
							draft &&
							typeof draft.content === "string" &&
							typeof draft.title === "string"
						) {
							this.pending.set(note.id, {
								title: draft.title.slice(0, 160),
								content: draft.content.slice(0, 20_000),
							});
						}
					}
				} catch {
					/* Ignore malformed recovery data. */
				}
			}
			const reconciled = notes.map((note) => {
				const current = this.base.get(note.id);
				if (this.running.has(note.id))
					return this.state.notes.find((item) => item.id === note.id) ?? note;
				const latest =
					current && current.revision > note.revision ? current : note;
				this.base.set(note.id, latest);
				return { ...latest, ...this.pending.get(note.id) };
			});
			this.emit({ notes: reconciled, loading: false, error: null });
			for (const id of this.pending.keys())
				if (!this.state.failed.includes(id))
					void this.flush(id).catch(() => {});
		} catch (error) {
			this.emit({ loading: false, error: String(error) });
		}
	}
	async create(input: CreateNote) {
		const note = await this.api.create(input);
		this.membershipVersion += 1;
		this.base.set(note.id, note);
		this.emit({ notes: [note, ...this.state.notes] });
		return note;
	}
	edit(id: string, patch: NotePatch) {
		if (this.deleting.has(id)) return;
		const note = this.state.notes.find((note) => note.id === id);
		if (!note) return;
		this.pending.set(id, { ...this.pending.get(id), ...patch });
		this.emit({
			notes: this.state.notes.map((note) =>
				note.id === id ? { ...note, ...patch } : note,
			),
			saving: [...new Set([...this.state.saving, id])],
		});
		this.backup();
		clearTimeout(this.timers.get(id));
		this.timers.set(
			id,
			setTimeout(() => {
				void this.flush(id).catch(() => {});
			}, 450),
		);
	}
	async flush(id: string): Promise<void> {
		if (this.deleting.has(id)) return;
		clearTimeout(this.timers.get(id));
		this.timers.delete(id);
		const active = this.running.get(id);
		if (active) {
			await active;
			return this.flush(id);
		}
		const patch = this.pending.get(id);
		const base = this.base.get(id);
		if (!patch || !base) return;
		this.emit({ saving: [...new Set([...this.state.saving, id])] });
		this.pending.delete(id);
		const operation = (async () => {
			try {
				const saved = await this.api.update({ ...base, ...patch });
				this.base.set(id, saved);
				this.emit({
					notes: this.state.notes.map((note) =>
						note.id === id ? { ...saved, ...this.pending.get(id) } : note,
					),
					failed: this.state.failed.filter((key) => key !== id),
				});
			} catch (error) {
				this.pending.set(id, { ...patch, ...this.pending.get(id) });
				this.emit({ failed: [...new Set([...this.state.failed, id])] });
				throw error;
			} finally {
				this.running.delete(id);
				this.emit({ saving: this.state.saving.filter((key) => key !== id) });
				this.backup();
			}
		})();
		this.running.set(id, operation);
		await operation;
		if (this.pending.has(id)) await this.flush(id);
	}
	async retry(id: string) {
		const fresh = (await this.api.list()).find((note) => note.id === id);
		if (!fresh) throw new Error("note_conflict");
		this.base.set(id, fresh);
		return this.flush(id);
	}
	async remove(ids: string[]) {
		for (const id of ids) this.deleting.add(id);
		try {
			for (const id of ids) {
				clearTimeout(this.timers.get(id));
				this.timers.delete(id);
				await this.running.get(id)?.catch(() => {});
			}
			await this.api.delete(ids);
			this.membershipVersion += 1;
			for (const id of ids) {
				this.pending.delete(id);
				this.base.delete(id);
			}
			this.emit({
				notes: this.state.notes.filter((note) => !ids.includes(note.id)),
				failed: this.state.failed.filter((id) => !ids.includes(id)),
				saving: this.state.saving.filter((id) => !ids.includes(id)),
			});
		} finally {
			for (const id of ids) this.deleting.delete(id);
			this.backup();
		}
	}
	flushAll() {
		return Promise.all(
			[...this.pending.keys(), ...this.running.keys()].map((id) =>
				this.flush(id),
			),
		);
	}
}
