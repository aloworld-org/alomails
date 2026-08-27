import { describe, expect, it, vi } from "vitest";

import { readQuoteImage } from "./quoteImageData";

describe("quote image reader", () => {
  it("passes a data URL to the callback", async () => {
    const done = vi.fn();
    readQuoteImage(new File(["image"], "image.png", { type: "image/png" }), done);
    await vi.waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(done.mock.calls[0]?.[0]).toMatch(/^data:image\/png;base64,/);
  });
});
