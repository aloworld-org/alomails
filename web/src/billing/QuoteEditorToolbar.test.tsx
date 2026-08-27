import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { QuoteEditorToolbar } from "./QuoteEditorToolbar";

afterEach(cleanup);

describe("QuoteEditorToolbar", () => {
  test("routes each available action and exposes the active mode", () => {
    const onEdit = vi.fn();
    const onCustomize = vi.fn();
    const onTogglePreview = vi.fn();
    render(
      <QuoteEditorToolbar
        creatingRevision={false}
        draft
        preview={false}
        onEdit={onEdit}
        onCustomize={onCustomize}
        onTogglePreview={onTogglePreview}
      />,
    );

    const edit = screen.getByRole("button", { name: strings.billingQuoteEdit });
    expect(edit.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(edit);
    fireEvent.click(screen.getByRole("button", { name: strings.quoteStudioCustomizeQuotation }));
    fireEvent.click(screen.getByRole("button", { name: strings.billingQuotationPreview }));

    expect(onEdit).toHaveBeenCalledOnce();
    expect(onCustomize).toHaveBeenCalledOnce();
    expect(onTogglePreview).toHaveBeenCalledOnce();
  });

  test("keeps revision actions visible but disabled while a revision is created", () => {
    render(
      <QuoteEditorToolbar
        creatingRevision
        draft={false}
        preview={false}
        onEdit={vi.fn()}
        onCustomize={vi.fn()}
        onTogglePreview={vi.fn()}
      />,
    );

    expect(
      (screen.getByRole("button", { name: strings.billingQuoteCreateRevisionAction }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("button", { name: strings.quoteStudioCustomizeQuotation }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
