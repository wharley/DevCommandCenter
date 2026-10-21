// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
	ReviewCardsOpenProvider,
	useReviewCardOpen,
	type ReviewCardsCommand,
} from "./review-cards-open";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Card({ id, forceOpen = false }: { id: string; forceOpen?: boolean }) {
	const [open, setOpen] = useReviewCardOpen(forceOpen);
	return (
		<button type="button" data-card={id} data-open={open} onClick={() => setOpen((v) => !v)}>
			{id}
		</button>
	);
}

let container: HTMLDivElement;
let root: Root;

function render(defaultOpen: boolean, command: ReviewCardsCommand | null, selected = "") {
	act(() => {
		root.render(
			<ReviewCardsOpenProvider value={{ defaultOpen, command }}>
				<Card id="a" forceOpen={selected === "a"} />
				<Card id="b" forceOpen={selected === "b"} />
			</ReviewCardsOpenProvider>,
		);
	});
}

const openState = () =>
	[...container.querySelectorAll("[data-card]")].map((el) => el.getAttribute("data-open"));

beforeEach(() => {
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});

afterEach(() => {
	act(() => root.unmount());
	container.remove();
});

describe("useReviewCardOpen", () => {
	it("starts from the list default", () => {
		render(false, null);
		expect(openState()).toEqual(["false", "false"]);
	});

	it("lets one card open on its own", () => {
		render(false, null);
		act(() => container.querySelector<HTMLElement>('[data-card="a"]')?.click());
		expect(openState()).toEqual(["true", "false"]);
	});

	it("follows expand all and collapse all, even when repeated", () => {
		render(false, null);
		render(false, { open: true, version: 1 });
		expect(openState()).toEqual(["true", "true"]);
		act(() => container.querySelector<HTMLElement>('[data-card="a"]')?.click());
		render(false, { open: false, version: 2 });
		expect(openState()).toEqual(["false", "false"]);
		act(() => container.querySelector<HTMLElement>('[data-card="b"]')?.click());
		render(false, { open: false, version: 3 });
		expect(openState()).toEqual(["false", "false"]);
	});

	it("opens a selected card in a collapsed list", () => {
		render(false, null, "b");
		expect(openState()).toEqual(["false", "true"]);
	});
});
