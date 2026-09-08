// Task navigation and persistence with real note components and isolated fixture IPC.
import assert from "node:assert/strict";
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const browser = await chromium.launch({
	headless: true,
	...(process.env.DCC_CHROMIUM_EXECUTABLE
		? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE }
		: {}),
});
const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
page.setDefaultTimeout(10000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const balloon = page.locator('[data-note-id="note-login"]');
const switchTask = () =>
	page.getByRole("button", { name: "Trocar tarefa", exact: true }).click();
try {
	await page.goto(
		`${process.env.DCC_NOTES_URL || "http://127.0.0.1:1432"}/tests/notes.html?restore`,
	);
	await balloon.waitFor();
	assert.equal(
		await page.locator(".note-balloon").count(),
		1,
		"only current task notes restore",
	);
	assert.equal(
		await page.evaluate(() =>
			Boolean(document.activeElement?.closest(".note-balloon")),
		),
		false,
		"automatic restoration does not steal focus",
	);
	const handle = balloon.getByRole("button", {
		name: "Arraste para mover · setas para ajustar a posição",
	});
	await handle.focus();
	await page.keyboard.press("Shift+ArrowLeft");
	await balloon
		.getByRole("button", { name: "Recolher para uma bolha" })
		.click();
	await page.waitForTimeout(350);
	const before = await balloon.evaluate((el) => ({
		left: el.style.left,
		top: el.style.top,
	}));
	await switchTask();
	await balloon.waitFor({ state: "hidden" });
	await switchTask();
	await page.locator('[data-note-id="note-login"].note-balloon-mini').waitFor();
	assert.deepEqual(
		await balloon.evaluate((el) => ({
			left: el.style.left,
			top: el.style.top,
		})),
		before,
	);
	await page.reload(); // fixture list deliberately takes 500ms; saved positions must survive loading
	await page.locator('[data-note-id="note-login"].note-balloon-mini').waitFor();
	assert.deepEqual(
		await balloon.evaluate((el) => ({
			left: el.style.left,
			top: el.style.top,
		})),
		before,
	);
	await balloon.getByRole("button", { name: "Expandir anotação" }).click();
	await balloon
		.getByRole("button", { name: "Fechar balão e manter anotação salva" })
		.click();
	await balloon.waitFor({ state: "hidden" });
	await page.getByRole("button", { name: "Anotações", exact: true }).click();
	await page.waitForTimeout(600);
	await page.keyboard.press("Escape");
	await page.waitForTimeout(300);
	assert.equal(
		await balloon.count(),
		0,
		"a library refresh does not undo dismissal for this visit",
	);
	await switchTask();
	await switchTask();
	await balloon.waitFor();
	await balloon
		.getByRole("button", { name: "Manter visível em outros projetos" })
		.click();
	await balloon.getByText("Salvo no projeto", { exact: true }).waitFor();
	await switchTask();
	assert.equal(
		await balloon.count(),
		1,
		"pinned notes stay visible in another task",
	);
	await switchTask();
	await balloon
		.getByRole("button", { name: "Mostrar apenas neste projeto" })
		.click();
	await balloon.getByText("Salvo no projeto", { exact: true }).waitFor();
	// Simulate a task created from the note; its implementation link must restore there too.
	await page.evaluate(() => {
		const notes = JSON.parse(localStorage.getItem("fixture.notes"));
		notes.find((note) => note.id === "note-login").implementationTaskId =
			"workspace-notes";
		localStorage.setItem("fixture.notes", JSON.stringify(notes));
	});
	await page.reload();
	await balloon.waitFor();
	await switchTask();
	await balloon.waitFor();
	await balloon.getByRole("button", { name: "Marcar como concluída" }).click();
	await balloon.waitFor({ state: "hidden" });
	await switchTask();
	await switchTask();
	await page.waitForTimeout(300);
	assert.equal(await balloon.count(), 0, "completed notes do not restore");
	await page.reload();
	await page.waitForTimeout(800);
	assert.equal(
		await balloon.count(),
		0,
		"completed notes stay hidden after reload",
	);
	assert.deepEqual(errors, []);
	console.log(
		"PASS: automatic restoration, task isolation, position and minimized state, delayed initial load, visit dismissal, pinning, implementation-task link, completion and focus preservation.",
	);
} finally {
	await browser.close();
}
