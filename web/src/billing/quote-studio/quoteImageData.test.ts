import { afterEach, describe, expect, it, vi } from "vitest";

import {
  QUOTE_IMAGE_MAX_SIDE,
  quoteImageTarget,
  readQuoteImage,
} from "./quoteImageData";

/** A stand-in for the browser's image decoder: reports the given size, or
 *  fails to decode. jsdom decodes nothing, so the test says what it would. */
function stubImage(decoded: { width: number; height: number } | null) {
  vi.stubGlobal(
    "Image",
    class {
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      naturalWidth = decoded?.width ?? 0;
      naturalHeight = decoded?.height ?? 0;
      set src(_value: string) {
        queueMicrotask(() => (decoded === null ? this.onerror : this.onload)?.());
      }
    },
  );
}

describe("quoteImageTarget", () => {
  it("keeps a picture that already fits", () => {
    expect(quoteImageTarget(800, 600)).toEqual({ width: 800, height: 600 });
    expect(quoteImageTarget(QUOTE_IMAGE_MAX_SIDE, 10)).toEqual({
      width: QUOTE_IMAGE_MAX_SIDE,
      height: 10,
    });
  });

  it("scales a large picture down to the cap on its longest side", () => {
    expect(quoteImageTarget(4000, 3000)).toEqual({ width: 1600, height: 1200 });
    expect(quoteImageTarget(1000, 5000)).toEqual({ width: 320, height: 1600 });
  });

  it("never produces an empty picture", () => {
    expect(quoteImageTarget(0, 0)).toEqual({ width: 1, height: 1 });
    expect(quoteImageTarget(1, 100000)).toEqual({ width: 1, height: 1600 });
  });
});

describe("readQuoteImage", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("stores a small JPEG exactly as chosen", async () => {
    stubImage({ width: 640, height: 480 });
    const done = vi.fn();
    readQuoteImage(new File(["image"], "photo.jpg", { type: "image/jpeg" }), done);
    await vi.waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(done.mock.calls[0]?.[0]).toMatch(/^data:image\/jpeg;base64,/);
  });

  it("still hands back a data URL when the picture cannot be decoded", async () => {
    stubImage(null);
    const done = vi.fn();
    readQuoteImage(new File(["image"], "image.png", { type: "image/png" }), done);
    await vi.waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(done.mock.calls[0]?.[0]).toMatch(/^data:image\/png;base64,/);
  });
});
