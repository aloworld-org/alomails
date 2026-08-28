// The responsive audit as a test (docs/autonomy/responsive R3).
//
// This is the sweep that found R1, made repeatable: sign in to the real app,
// visit every module at phone / tablet / laptop / desktop widths, and hold the
// three layout invariants —
//   1. the document never scrolls horizontally,
//   2. no element is wider than the viewport, unless it sits inside a strip
//      that scrolls on purpose and says so with `data-allow-overflow`,
//   3. a module's own sidebar is a closed drawer at phone widths, never a
//      column squeezing the content (the R1 bug).
//
// One test per viewport, walking all modules in one signed-in page: a fresh
// browser context per module would replay the rotated refresh token and trip
// the server's replay-chain revocation, so the walk shares one session the way
// a person does. Violations are soft assertions — one bad module must not hide
// the others — and each failing module leaves a screenshot in
// e2e/.artifacts/.
import { expect, test, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import path from "node:path";

import { en } from "../src/i18n/en";
import { ADMIN_EMAIL, ADMIN_PASSWORD, ARTIFACTS_DIR } from "./stack";

const VIEWPORTS = [
  { width: 360, height: 740 },
  { width: 768, height: 1024 },
  { width: 1024, height: 768 },
  { width: 1440, height: 900 },
];

/** Every module the workplace surface mounts (product/workplace.tsx order).
 *  The admin and control consoles are operator surfaces, not modules, and are
 *  not in this sweep. */
const MODULES = [
  "/home",
  "/mail",
  "/agenda",
  "/tasks",
  "/drive",
  "/billing",
  "/crm",
  "/projects",
  "/finance",
  "/inventory",
  "/hr",
  "/insights",
  "/sites",
  "/campaigns",
  "/chat",
  "/meet",
];

/** Modules whose sidebar is the ds ModuleSidebar drawer, with the toggle's
 *  accessible name (both label states, so a mislabelled-but-working toggle
 *  still identifies itself). */
const DRAWER_MODULES: Record<string, RegExp> = {
  "/mail": new RegExp(`^(${en.expandFolders}|${en.collapseFolders})$`),
  "/tasks": new RegExp(`^(${en.taskShowProjects}|${en.taskHideProjects})$`),
};

/** `useIsMobile` treats ≤768px as a phone (the established convention). */
const MOBILE_MAX = 768;

interface WidthViolation {
  tag: string;
  width: number;
  hint: string;
}

async function signIn(page: Page): Promise<void> {
  await page.goto("/login");
  await page.locator('input[type="email"]').fill(ADMIN_EMAIL);
  await page.locator('input[type="password"]').fill(ADMIN_PASSWORD);
  await page.locator('form button[type="submit"]').click();
  await page.waitForURL("**/mail", { timeout: 30_000 });
}

/** Navigate and wait until the module is actually on screen: the lazy-chunk
 *  skeleton gone, fonts loaded, two frames painted. */
async function openModule(page: Page, module: string): Promise<void> {
  await page.goto(module, { waitUntil: "domcontentloaded" });
  await page.locator("main").first().waitFor({ state: "visible", timeout: 30_000 });
  // Measure only once the module is really on screen: `main` has content and
  // the lazy-chunk skeleton (a role=status placeholder wider than a phone) is
  // gone. Checking for the skeleton's absence alone races the Suspense mount —
  // an empty `main` "has no skeleton" while the chunk is still on the wire,
  // and a blank page passes every width invariant. In-module spinners share
  // the role, so a module legitimately still spinning is not waited to death:
  // on timeout the sweep measures what is there.
  await page
    .waitForFunction(
      () => {
        const main = document.querySelector("main");
        if (main === null || main.children.length === 0) return false;
        return main.querySelector('[role="status"]') === null;
      },
      undefined,
      { timeout: 30_000 },
    )
    .catch(() => {});
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(resolve)),
    );
  });
}

/** The layout facts of the current page, measured in the real layout engine. */
async function measure(page: Page): Promise<{
  documentScrollWidth: number;
  viewportWidth: number;
  tooWide: WidthViolation[];
}> {
  return page.evaluate(() => {
    const viewportWidth = window.innerWidth;
    const tooWide: { tag: string; width: number; hint: string }[] = [];
    for (const el of Array.from(document.querySelectorAll("body *"))) {
      const rect = el.getBoundingClientRect();
      if (rect.width <= viewportWidth + 1) continue;
      // Strips that scroll on purpose declare themselves.
      if (el.closest("[data-allow-overflow]") !== null) continue;
      // Invisible elements occupy no user-facing layout.
      const style = getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") continue;
      tooWide.push({
        tag: el.tagName.toLowerCase(),
        width: Math.round(rect.width),
        hint: (el.getAttribute("class") ?? "").slice(0, 80),
      });
      if (tooWide.length >= 10) break;
    }
    return {
      documentScrollWidth: document.documentElement.scrollWidth,
      viewportWidth,
      tooWide,
    };
  });
}

for (const viewport of VIEWPORTS) {
  test.describe(`${viewport.width}px`, () => {
    test.use({ viewport });

    test(`every module holds the layout invariants at ${viewport.width}px`, async ({
      page,
    }) => {
      await signIn(page);
      mkdirSync(ARTIFACTS_DIR, { recursive: true });

      for (const module of MODULES) {
        await openModule(page, module);
        const facts = await measure(page);
        const failures: string[] = [];

        if (facts.documentScrollWidth > facts.viewportWidth) {
          failures.push(
            `document scrolls horizontally (${facts.documentScrollWidth} > ${facts.viewportWidth})`,
          );
        }
        for (const wide of facts.tooWide) {
          failures.push(
            `<${wide.tag} class="${wide.hint}"> is ${wide.width}px wide`,
          );
        }

        const toggleName = DRAWER_MODULES[module];
        if (toggleName !== undefined) {
          const toggle = page.getByRole("button", { name: toggleName });
          const drawer = page.getByRole("dialog");
          if (viewport.width <= MOBILE_MAX) {
            // Phone: the sidebar is a drawer — closed by default, reachable
            // through its toggle. A visible toggle proves the phone branch is
            // active (the desktop column never renders one).
            if ((await toggle.count()) === 0) {
              failures.push("sidebar drawer toggle is missing at a phone width");
            }
            if ((await drawer.count()) > 0) {
              failures.push("sidebar drawer is open before anyone asked");
            }
          } else if ((await drawer.count()) > 0) {
            failures.push("sidebar renders as a drawer at a desktop width");
          }
        }

        if (failures.length > 0) {
          const shot = path.join(
            ARTIFACTS_DIR,
            `${module.slice(1)}-${viewport.width}px.png`,
          );
          await page.screenshot({ path: shot, fullPage: false });
          failures.push(`screenshot: ${shot}`);
        }
        expect
          .soft(failures, `${module} at ${viewport.width}px`)
          .toEqual([]);
      }
    });
  });
}
