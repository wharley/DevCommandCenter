import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";

const { chromium } = await import(
	process.argv[2] || process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
page.setDefaultTimeout(15000);
await page.emulateMedia({ colorScheme: "dark" });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const output = "/tmp/dcc-turn-review-timeline";
const button = (name) => page.getByRole("button", { name, exact: true });
const oldCard = page.locator('[data-turn-review-id="turn-history"]');
const dialog = page.getByRole("dialog", { name: "Alterações desta execução" });
const shot = (name) => page.screenshot({ path: `${output}/${name}.png` });
try {
	await mkdir(output, { recursive: true });
	await page.goto(
		`${process.env.DCC_REVIEW_URL || "http://127.0.0.1:1432"}/tests/review-surfaces.html`,
	);
	await button("Timeline").click();
	await oldCard.waitFor();
	await page
		.getByRole("textbox", { name: "Mensagem" })
		.fill("Confira também o caso de erro.");
	assert.equal(
		await oldCard.locator(".dcc-turn-review-file-trigger").count(),
		3,
	);
	assert.equal(await dialog.count(), 0);
	assert.equal(
		await page.evaluate(
			() =>
				window.reviewRequests.filter(
					(r) => r.command === "turn_review_file_diff",
				).length,
		),
		0,
	);
	await shot("card-dark");
	await oldCard
		.getByRole("button", { name: "Mais 1 arquivo", exact: true })
		.click();
	assert.equal(
		await oldCard.locator(".dcc-turn-review-file-trigger").count(),
		4,
	);
	const trigger = oldCard.getByRole("button", {
		name: "Revisar alterações",
		exact: true,
	});
	await trigger.focus();
	await page.keyboard.press("Enter");
	await dialog.getByRole("code").waitFor();
	assert.equal(
		await dialog
			.getByRole("button", { name: "Arquivo anterior", exact: true })
			.isDisabled(),
		true,
	);
	await shot("popup-dark");
	await button("Próximo arquivo").click();
	await dialog
		.getByRole("region", {
			name: "src/components/composer/use-prefill.ts",
			exact: true,
		})
		.waitFor();
	assert.equal(await dialog.locator("[data-turn-review-diff]").count(), 1);
	await button("Próximo arquivo").click();
	await dialog.getByRole("code").waitFor();
	assert.match(
		await dialog.getByRole("code").innerText(),
		/Keep the composer draft/,
	);
	await page.waitForFunction(() =>
		window.reviewRequests.some(
			(r) =>
				r.command === "turn_review_file_diff" &&
				r.args.input.snapshotId === "snapshot-history" &&
				r.args.input.path === "docs/review-notes.md",
		),
	);
	await shot("new-file");
	await page.keyboard.press("Escape");
	await dialog.waitFor({ state: "hidden" });
	assert.equal(
		await trigger.evaluate((el) => document.activeElement === el),
		true,
	);
	assert.equal(
		await page.getByRole("textbox", { name: "Mensagem" }).inputValue(),
		"Confira também o caso de erro.",
	);
	await button("Tema").click();
	await page.setViewportSize({ width: 390, height: 844 });
	await oldCard.locator('button[title="assets/preview.png"]').click();
	await dialog
		.getByText("Prévia indisponível nesta execução.", { exact: true })
		.waitFor();
	assert.equal(await button("Próximo arquivo").isDisabled(), true);
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth <= innerWidth,
		),
		true,
	);
	await shot("narrow-light");
	await button("Alterações atuais").click();
	await dialog.waitFor({ state: "hidden" });
	await page
		.getByRole("complementary", { name: "Inspector de revisão" })
		.getByRole("button", { name: "Workspace", exact: true })
		.waitFor();
	assert.deepEqual(errors, []);
	console.log(
		JSON.stringify({
			ok: true,
			screenshots: output,
			checks: [
				"visible files",
				"lazy patches",
				"keyboard",
				"historical diff",
				"new file",
				"single selected diff",
				"focus restored",
				"draft preserved",
				"light/dark",
				"narrow layout",
				"current inspector",
			],
		}),
	);
} finally {
	await browser.close();
}
