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
const shot = (name) => page.screenshot({ path: `${output}/${name}.png` });
try {
	await mkdir(output, { recursive: true });
	await page.goto(
		`${process.env.DCC_REVIEW_URL || "http://127.0.0.1:1432"}/tests/review-surfaces.html`,
	);
	await button("Timeline").click();
	await page.waitForFunction(
		() => document.querySelectorAll(".dcc-turn-review-card").length === 2,
	);
	await page
		.getByRole("textbox", { name: "Mensagem" })
		.fill("Confira também o caso de erro.");
	assert.equal(await page.locator(".dcc-turn-review-file-trigger").count(), 0);
	await page.waitForTimeout(500);
	assert.equal(
		await page.locator("html").evaluate((el) => el.classList.contains("dark")),
		true,
	);
	await shot("closed-dark");
	await page.evaluate(() => {
		window.reviewRequests = [];
	});
	await oldCard
		.getByRole("button", { name: "Ver arquivos", exact: true })
		.focus();
	await page.keyboard.press("Enter");
	assert.equal(
		await oldCard
			.locator('.dcc-turn-review-file-trigger[aria-expanded="false"]')
			.count(),
		3,
	);
	assert.equal(
		await page.evaluate(
			() =>
				window.reviewRequests.filter(
					(r) => r.command === "turn_review_file_diff",
				).length,
		),
		0,
	);
	const first = oldCard.locator('button[title="src/review.ts"]');
	const before = await first.boundingBox();
	await first.click();
	await oldCard.locator(".dcc-turn-review-preview").waitFor();
	await page.waitForTimeout(500);
	const after = await first.boundingBox();
	assert.ok(
		Math.abs(before.y - after.y) < 3,
		"expanding a diff preserves the reading position",
	);
	await oldCard
		.locator('button[title="src/components/composer/use-prefill.ts"]')
		.click();
	assert.equal(await oldCard.locator(".dcc-turn-review-preview").count(), 1);
	assert.equal(await first.getAttribute("aria-expanded"), "false");
	await page.waitForFunction(() =>
		window.reviewRequests.some(
			(r) =>
				r.command === "turn_review_file_diff" &&
				r.args.input.snapshotId === "snapshot-history" &&
				r.args.input.path === "src/components/composer/use-prefill.ts",
		),
	);
	await page.waitForTimeout(500);
	await shot("inline-dark");
	await oldCard
		.getByRole("button", { name: "Revisar alterações", exact: true })
		.click();
	const inspector = page.getByRole("complementary", {
		name: "Inspector de revisão",
	});
	await inspector.getByText("Execução selecionada", { exact: true }).waitFor();
	const selected = inspector.locator('[data-review-selected="true"]');
	await selected.waitFor();
	assert.equal(
		await selected.getAttribute("data-review-file"),
		"src/components/composer/use-prefill.ts",
	);
	assert.equal(
		await page.getByRole("textbox", { name: "Mensagem" }).inputValue(),
		"Confira também o caso de erro.",
	);
	await inspector.getByText("Não há preview de texto capturado para este arquivo.", { exact: true }).waitFor();
	await shot("inspector-history");
	await inspector
		.getByRole("button", { name: "Alterações atuais", exact: true })
		.click();
	await inspector
		.getByRole("button", { name: "Workspace", exact: true })
		.waitFor();
	await button("Fechar inspector").click();
	await button("Tema").click();
	await page.waitForTimeout(300);
	await shot("inline-light");
	await oldCard
		.getByRole("button", { name: "Ocultar arquivos", exact: true })
		.click();
	await oldCard
		.getByRole("button", { name: "Ver arquivos", exact: true })
		.click();
	assert.equal(await oldCard.locator(".dcc-turn-review-preview").count(), 0);
	await page.setViewportSize({ width: 720, height: 900 });
	await oldCard.locator('button[title="assets/preview.png"]').click();
	await oldCard
		.getByText("Não há preview de texto capturado para este arquivo.", {
			exact: true,
		})
		.waitFor();
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth <= innerWidth,
		),
		true,
	);
	await shot("narrow-light");
	assert.deepEqual(errors, []);
	console.log(
		JSON.stringify({
			ok: true,
			screenshots: output,
			checks: [
				"collapsed default",
				"keyboard",
				"lazy patches",
				"single open diff",
				"reading position",
				"historical inspector",
				"selected file",
				"draft preserved",
				"light/dark",
				"narrow layout",
			],
		}),
	);
} finally {
	await browser.close();
}
