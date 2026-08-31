import { expect, test } from "vitest";

import { MAX_LOGO_BYTES } from "./model";
import { readLogoFile, validateLogoFile } from "./logoFiles";

test("logo files accept supported images within the upload limit", () => {
  expect(validateLogoFile(new File(["logo"], "mark.png", { type: "image/png" }))).toBeNull();
  expect(validateLogoFile(new File(["logo"], "mark.txt", { type: "text/plain" }))).toBe("unsupported");
  expect(validateLogoFile(new File([new Uint8Array(MAX_LOGO_BYTES + 1)], "mark.webp", { type: "image/webp" }))).toBe("too-large");
});

test("logo files accept safe SVG artwork and reject executable markup", async () => {
  const safe = new File(['<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h10v10z"/></svg>'], "mark.svg", { type: "image/svg+xml" });
  const unsafe = new File(['<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>'], "bad.svg", { type: "image/svg+xml" });

  expect(validateLogoFile(safe)).toBeNull();
  await expect(readLogoFile(safe, "safe-logo")).resolves.toEqual(expect.objectContaining({ mimeType: "image/svg+xml" }));
  await expect(readLogoFile(unsafe, "unsafe-logo")).rejects.toThrow("unsafe-svg");
});
