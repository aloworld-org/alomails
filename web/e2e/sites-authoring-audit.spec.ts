import { expect, test, type Page } from "@playwright/test";

import { en } from "../src/i18n/en";
import { ADMIN_EMAIL, ADMIN_PASSWORD } from "./stack";

async function signIn(page: Page): Promise<void> {
  await page.goto("/login");
  await page.locator('input[type="email"]').fill(ADMIN_EMAIL);
  await page.locator('input[type="password"]').fill(ADMIN_PASSWORD);
  await page.locator('form button[type="submit"]').click();
  await page.waitForURL("**/mail", { timeout: 30_000 });
}

test("a person can build and edit a complete website", async ({
  page,
  context,
}) => {
  const consoleErrors: string[] = [];
  const failedRequests: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("requestfailed", (request) => {
    failedRequests.push(
      `${request.method()} ${request.url()} — ${request.failure()?.errorText ?? "failed"}`,
    );
  });

  await signIn(page);
  await page.goto("/sites");
  await page.getByRole("button", { name: en.sitesNewSite }).click();
  await page.getByRole("button", { name: en.sitesTemplateChoice }).click();
  await page.getByLabel(en.sitesFieldName).fill("Northstar Studio");
  await page.getByLabel(en.sitesFieldSubdomain).fill("northstar-studio-audit");
  await expect(page.getByText(en.sitesAddressAvailable)).toBeVisible({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: en.sitesCreateSite }).click();
  await page.waitForURL(/\/sites\/[^/]+\/pages\/[^/]+/, { timeout: 30_000 });

  await expect(
    page.getByRole("button", { name: en.sitesAddSection }),
  ).toBeVisible();
  await page.getByRole("button", { name: en.sitesAddSection }).click();
  const palette = page.getByRole("dialog", { name: en.sitesPaletteTitle });
  await page.locator('[data-palette-tile="hero"]').click();
  await palette.waitFor({ state: "hidden", timeout: 10_000 });
  const dialog = page.getByRole("dialog", {
    name: new RegExp(`(Add|Edit) ${en.sitesSectionHero}`),
  });
  if (!(await dialog.isVisible().catch(() => false))) {
    const editHero = page.getByRole("button", {
      name: en.sitesEditSection(en.sitesSectionHero),
    });
    await expect(editHero).toBeEnabled({ timeout: 15_000 });
    await editHero.click();
  }
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  await dialog
    .getByRole("textbox", { name: en.sitesFieldHeading, exact: true })
    .fill("Thoughtful spaces, beautifully made", { timeout: 10_000 });
  await dialog
    .getByRole("textbox", { name: en.sitesFieldSubheading, exact: true })
    .fill("Calm interior design for homes and independent businesses.", {
      timeout: 10_000,
    });
  const heroUpload = dialog.locator('input[type="file"]').first();
  await heroUpload.setInputFiles({
    name: "northstar-hero.png",
    mimeType: "image/png",
    buffer: Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
      "base64",
    ),
  });
  await expect(
    dialog.getByRole("textbox", { name: en.sitesFieldImageId, exact: true }),
  ).not.toHaveValue("", { timeout: 30_000 });
  await dialog.getByRole("button", { name: en.sitesSaveSection }).click();
  await expect(
    page.getByText("Thoughtful spaces, beautifully made"),
  ).toBeVisible();

  for (const kind of [
    "features",
    "testimonials",
    "faq",
    "cta",
    "contact_form",
  ] as const) {
    await test.step(`add ${kind}`, async () => {
      await page
        .getByRole("button", { name: en.sitesAddSection })
        .click({ timeout: 10_000 });
      const sectionPalette = page.getByRole("dialog", {
        name: en.sitesPaletteTitle,
      });
      await page.locator(`[data-palette-tile="${kind}"]`).click();
      await sectionPalette.waitFor({ state: "hidden", timeout: 10_000 });
      const form = page.getByRole("dialog");
      if (await form.isVisible().catch(() => false)) {
        if (kind === "features") {
          await form
            .getByRole("textbox", { name: en.sitesFieldHeading, exact: true })
            .fill("What we design");
          await form
            .getByRole("textbox", { name: en.sitesFieldItemTitle, exact: true })
            .fill("Quiet confidence");
          await form
            .getByRole("textbox", { name: en.sitesFieldBody, exact: true })
            .fill("Spaces that feel composed, useful, and distinctly yours.");
        } else if (kind === "testimonials") {
          await form
            .getByRole("textbox", { name: en.sitesFieldQuote, exact: true })
            .fill("Northstar made every decision feel simple.");
          await form
            .getByRole("textbox", { name: en.sitesFieldAuthor, exact: true })
            .fill("Maya Chen");
        } else if (kind === "faq") {
          await form
            .getByRole("textbox", { name: en.sitesFieldQuestion, exact: true })
            .fill("How does a project begin?");
          await form
            .getByRole("textbox", { name: en.sitesFieldAnswer, exact: true })
            .fill(
              "We begin with a focused conversation about your space and priorities.",
            );
        } else if (kind === "cta") {
          await form
            .getByRole("textbox", { name: en.sitesFieldHeading, exact: true })
            .fill("Ready to shape your space?");
          await form
            .getByRole("textbox", { name: en.sitesFieldLinkLabel, exact: true })
            .fill("Start a conversation");
          await form
            .getByRole("textbox", { name: en.sitesFieldLinkHref, exact: true })
            .fill("#contact");
        }
        const save = form.getByRole("button", { name: en.sitesSaveSection });
        if (await save.isEnabled()) await save.click();
        else
          await form
            .getByRole("button", { name: en.close, exact: true })
            .click();
        await form.waitFor({ state: "hidden", timeout: 10_000 });
      }
    });
  }

  await page
    .getByRole("button", { name: en.sitesMoveDown(en.sitesSectionHero) })
    .click();
  const undo = page.getByRole("button", { name: en.sitesUndoEdit });
  await expect(undo).toBeEnabled({ timeout: 15_000 });
  await undo.focus();
  await page.keyboard.press("Enter");

  const themeButton = page.getByRole("button", { name: en.sitesTheme });
  await themeButton.evaluate((button) => {
    for (
      let parent = button.parentElement;
      parent !== null;
      parent = parent.parentElement
    ) {
      if (parent.scrollHeight > parent.clientHeight) parent.scrollTop = 0;
    }
  });
  await test.step("open and apply theme", async () => {
    await themeButton.focus();
    await page.keyboard.press("Enter");
    const themeDialog = page.getByRole("dialog", { name: en.sitesThemeTitle });
    await expect(themeDialog).toBeVisible({ timeout: 10_000 });
    await themeDialog
      .getByRole("button", { name: en.sitesThemeApply })
      .click({ timeout: 10_000 });
    await themeDialog.waitFor({ state: "hidden", timeout: 15_000 });
  });

  const previewButton = page.getByRole("button", { name: en.sitesShowPreview });
  await previewButton.focus();
  await page.keyboard.press("Enter");
  const preview = page.frameLocator("iframe");
  await expect(
    preview.getByText("Thoughtful spaces, beautifully made"),
  ).toBeVisible({ timeout: 15_000 });

  const [browserPreview] = await Promise.all([
    context.waitForEvent("page"),
    page.getByRole("button", { name: en.sitesPreviewInBrowser }).click(),
  ]);
  await expect(
    browserPreview.getByText("Thoughtful spaces, beautifully made"),
  ).toBeVisible({ timeout: 15_000 });
  await browserPreview.close();

  const heroImage = preview.locator(".s-hero img").first();
  await expect(heroImage).toBeVisible({ timeout: 15_000 });
  await heroImage.dblclick();
  await preview.getByRole("button", { name: en.sitesImageFrameLeft }).click();
  await expect(
    preview.getByRole("button", { name: en.sitesImageFrameTop }),
  ).toBeVisible({ timeout: 15_000 });
  await preview.getByRole("button", { name: en.sitesImageFrameTop }).click();
  await preview
    .getByRole("button", { name: en.close, exact: true })
    .click({ timeout: 10_000 });

  const editableHeading = preview
    .locator('[data-alo-text*="/heading"]')
    .first();
  await editableHeading.click({ timeout: 10_000 });
  await editableHeading.press("Control+A");
  await editableHeading.fill("Northstar spaces, made for living");
  await editableHeading.press("Enter");
  await expect(
    preview.getByText("Northstar spaces, made for living"),
  ).toBeVisible({ timeout: 15_000 });
  await editableHeading.evaluate((heading) => {
    const text = heading.firstChild;
    if (text === null) throw new Error("heading has no text node");
    const range = document.createRange();
    range.setStart(text, 0);
    range.setEnd(text, Math.min(9, text.textContent?.length ?? 0));
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
  });
  await preview.getByRole("button", { name: en.bold }).click();
  await expect
    .poll(() =>
      editableHeading.evaluate((heading) =>
        /font-weight|<b>|<strong>/.test(heading.innerHTML),
      ),
    )
    .toBe(true);

  await page.screenshot({
    path: "e2e/.artifacts/sites-authoring-desktop.png",
    fullPage: true,
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await page.locator("main").waitFor({ state: "visible" });
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
  await page.screenshot({
    path: "e2e/.artifacts/sites-authoring-mobile.png",
    fullPage: true,
  });

  expect(failedRequests, failedRequests.join("\n")).toEqual([]);
  expect(consoleErrors, consoleErrors.join("\n")).toEqual([]);
});
