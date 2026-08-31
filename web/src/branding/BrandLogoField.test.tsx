import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BrandLogoField } from "./BrandLogoField";
import { MAX_LOGO_BYTES } from "./model";

afterEach(cleanup);

test("the entire logo drop zone opens the image picker", () => {
  const { container } = render(<BrandLogoField logos={[]} primaryLogoId={null} onChange={vi.fn()} />);
  const input = container.querySelector("input[type='file']") as HTMLInputElement;
  const click = vi.spyOn(input, "click");

  fireEvent.click(screen.getByRole("button", { name: new RegExp(strings.brandingLogoDropTitle) }));

  expect(click).toHaveBeenCalledOnce();
  expect(input.multiple).toBe(true);
});

test("logo upload explains unsupported and oversized files", () => {
  const { container } = render(<BrandLogoField logos={[]} primaryLogoId={null} onChange={vi.fn()} />);
  const input = container.querySelector("input[type='file']") as HTMLInputElement;
  fireEvent.change(input, { target: { files: [new File(["text"], "logo.txt", { type: "text/plain" })] } });
  expect(screen.getByRole("alert").textContent).toBe(strings.brandingLogoUnsupported);
  fireEvent.change(input, { target: { files: [new File([new Uint8Array(MAX_LOGO_BYTES + 1)], "logo.png", { type: "image/png" })] } });
  expect(screen.getByRole("alert").textContent).toBe(strings.brandingLogoTooLarge);
});

test("dropping multiple logos appends them and selects the first as primary", async () => {
  const onChange = vi.fn();
  render(<BrandLogoField logos={[]} primaryLogoId={null} onChange={onChange} />);
  const zone = screen.getByRole("button", { name: new RegExp(strings.brandingLogoDropTitle) });

  fireEvent.drop(zone, {
    dataTransfer: {
      files: [
        new File(["first"], "original.png", { type: "image/png" }),
        new File(["second"], "white.webp", { type: "image/webp" }),
      ],
    },
  });

  await waitFor(() => expect(onChange).toHaveBeenCalledOnce());
  const [logos, primaryLogoId] = onChange.mock.calls[0] as [Array<{ id: string }>, string];
  expect(logos).toHaveLength(2);
  expect(primaryLogoId).toBe(logos[0]?.id);
});
