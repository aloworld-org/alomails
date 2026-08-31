import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { InvoiceEditorToolbar } from "./InvoiceEditorToolbar";

afterEach(cleanup);

describe("InvoiceEditorToolbar", () => {
  test("offers draft editing, customization, preview and PDF actions", () => {
    const actions = {
      onEdit: vi.fn(),
      onCustomize: vi.fn(),
      onTogglePreview: vi.fn(),
      onDownloadPdf: vi.fn(),
    };
    render(
      <InvoiceEditorToolbar
        draft
        preview={false}
        downloading={false}
        {...actions}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: strings.billingInvoiceEdit }));
    fireEvent.click(screen.getByRole("button", { name: strings.billingCustomizeInvoice }));
    fireEvent.click(screen.getByRole("button", { name: strings.billingInvoicePreview }));
    fireEvent.click(screen.getByRole("button", { name: strings.billingDownloadPdf }));
    expect(actions.onEdit).toHaveBeenCalledOnce();
    expect(actions.onCustomize).toHaveBeenCalledOnce();
    expect(actions.onTogglePreview).toHaveBeenCalledOnce();
    expect(actions.onDownloadPdf).toHaveBeenCalledOnce();
  });

  test("keeps finalized invoices previewable without edit controls", () => {
    render(
      <InvoiceEditorToolbar
        draft={false}
        preview
        downloading
        onEdit={vi.fn()}
        onCustomize={vi.fn()}
        onTogglePreview={vi.fn()}
        onDownloadPdf={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: strings.billingInvoiceEdit })).toBeNull();
    expect(screen.queryByRole("button", { name: strings.billingCustomizeInvoice })).toBeNull();
    expect(screen.getByRole("button", { name: strings.billingExitPreview })).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: strings.billingDownloadPdf }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
