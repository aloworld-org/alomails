// The funnel screen (S2.10c) and the handoff that fills it.
//
// What is pinned here is what an owner would be misled by if it broke: the
// four honesty properties of the report (evidence per step, per-form columns
// that are not addends, currencies never summed, the stated invoice rule), the
// three different absences (no CRM, no Billing, no form), and the one rule
// that makes the handoff worth having — that nothing the workspace already
// knows is asked for a second time.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesError } from "./api";
import { FunnelView } from "./FunnelView";
import { funnelMoney } from "./funnelReading";
import { SubmissionsView } from "./SubmissionsView";
import type { SiteAttributionReport, SiteLeadLink } from "./types";

const mocks = vi.hoisted(() => ({
  site: vi.fn(),
  attribution: vi.fn(),
  submissions: vi.fn(),
  siteLeads: vi.fn(),
  createSiteLead: vi.fn(),
  deleteSiteLead: vi.fn(),
  setSubmissionHandled: vi.fn(),
  crmBoards: vi.fn(),
  crmColumns: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

vi.mock("../platform/download", () => ({ saveTextFile: vi.fn() }));

function report(overrides: Partial<SiteAttributionReport> = {}): SiteAttributionReport {
  return {
    from: "2026-07-14",
    to: "2026-08-12",
    invoiceRule: "customerSinceLead",
    billingVisible: true,
    totals: {
      views: 120,
      starts: 44,
      submits: 18,
      leads: 6,
      dealsOpen: 3,
      dealsWon: 2,
      dealsLost: 1,
      invoices: 2,
      money: [{ currency: "EUR", openCents: 250_000, wonCents: 180_000, invoicedCents: 217_800 }],
    },
    sources: [
      {
        kind: "form",
        id: "form-1",
        name: "Contact",
        views: 100,
        starts: 40,
        submits: 16,
        leads: 6,
        dealsOpen: 3,
        dealsWon: 2,
        dealsLost: 1,
        invoices: 2,
        money: [{ currency: "EUR", openCents: 250_000, wonCents: 180_000, invoicedCents: 217_800 }],
      },
      {
        kind: "form",
        id: "form-2",
        name: null,
        views: 20,
        starts: 4,
        submits: 2,
        leads: 0,
        dealsOpen: 0,
        dealsWon: 0,
        dealsLost: 0,
        invoices: 0,
        money: [],
      },
    ],
    ...overrides,
  };
}

function submission(overrides: Record<string, unknown> = {}) {
  return {
    id: "sub-1",
    formId: "form-1",
    formName: "Contact",
    senderName: "Ada Lovelace",
    senderEmail: "ada@visitor.example",
    message: "Do you deliver to Ghent?",
    handled: false,
    receivedAt: "2026-08-10T09:15:00Z",
    ...overrides,
  };
}

function link(overrides: Partial<SiteLeadLink> = {}): SiteLeadLink {
  return {
    id: "link-1",
    siteId: "site-1",
    sourceKind: "form",
    sourceId: "form-1",
    submissionId: "sub-1",
    linkedBy: "user-1",
    linkedAt: "2026-08-10T10:00:00Z",
    deal: {
      id: "deal-1",
      title: "Website enquiry — Ada Lovelace",
      valueCents: 250_000,
      currency: "EUR",
      state: "open",
    },
    ...overrides,
  };
}

function renderFunnel() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/funnel"]}>
      <Routes>
        <Route path="/sites/:siteId/funnel" element={<FunnelView />} />
        <Route path="/sites/:siteId" element={<p>site home</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

function renderInbox() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/submissions"]}>
      <Routes>
        <Route path="/sites/:siteId/submissions" element={<SubmissionsView />} />
        <Route path="/sites/:siteId" element={<p>site home</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

/** The panel a heading names, so an assertion cannot match a number that
 *  belongs to the panel beside it. */
function panel(title: string): HTMLElement {
  const heading = screen.getByRole("heading", { name: title });
  const section = heading.closest("section");
  if (section === null) throw new Error(`no panel around ${title}`);
  return section;
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.site.mockResolvedValue({
    id: "site-1",
    name: "Axon",
    subdomain: "axon",
    status: "live",
  });
  mocks.attribution.mockResolvedValue(report());
  mocks.submissions.mockResolvedValue([submission()]);
  mocks.siteLeads.mockResolvedValue([]);
  mocks.crmBoards.mockResolvedValue([
    { id: "board-1", name: "Sales", archived: false },
  ]);
  mocks.crmColumns.mockResolvedValue([
    { id: "stage-won", pipelineId: "board-1", name: "Won", position: 3, closed: true, archived: false },
    { id: "stage-new", pipelineId: "board-1", name: "New", position: 0, closed: false, archived: false },
  ]);
  mocks.createSiteLead.mockResolvedValue(link());
  mocks.deleteSiteLead.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("the funnel", () => {
  test("draws the chain and says where each step's number comes from", async () => {
    renderFunnel();
    await screen.findByText(strings.sitesFunnelChain);

    const chain = panel(strings.sitesFunnelChain);
    const steps = within(chain).getAllByRole("listitem");
    expect(steps.map((step) => step.firstElementChild?.textContent)).toEqual([
      strings.sitesFunnelStageViews,
      strings.sitesFunnelStageStarts,
      strings.sitesFunnelStageSubmits,
      strings.sitesFunnelStageLeads,
      strings.sitesFunnelStageWon,
      strings.sitesFunnelStageInvoices,
    ]);
    // The two browser-reported steps are marked as such, and the four counted
    // at the write are marked as those. The floor note says why it matters.
    expect(within(chain).getAllByText(strings.sitesFunnelFromBrowser).length).toBe(2);
    expect(within(chain).getAllByText(strings.sitesFunnelFromRecord).length).toBe(4);
    expect(within(chain).getByText(strings.sitesFunnelFloorNote)).toBeTruthy();
  });

  test("a step can be larger than the one before it without breaking the bars", async () => {
    // Independent counters: an anchor arrival or a lost beacon can leave more
    // starts than views. Bars are drawn against the largest step, so the
    // longest bar is 100% and nothing exceeds it.
    mocks.attribution.mockResolvedValue(
      report({
        totals: { ...report().totals, views: 4, starts: 9 },
      }),
    );
    renderFunnel();
    await screen.findByText(strings.sitesFunnelChain);
    const bars = panel(strings.sitesFunnelChain).querySelectorAll("[style*='--funnel-share']");
    const shares = [...bars].map((bar) =>
      Number(bar.getAttribute("style")?.match(/--funnel-share:\s*([\d.]+)/u)?.[1] ?? "0"),
    );
    expect(Math.max(...shares)).toBe(1);
    expect(shares[0]).toBeLessThan(shares[1] ?? 0);
  });

  test("names the money per currency, with the rule that produced it", async () => {
    renderFunnel();
    await screen.findByText(strings.sitesFunnelChain);
    const money = panel(strings.sitesFunnelMoney);
    expect(within(money).getByText(strings.sitesFunnelInvoiceRule)).toBeTruthy();
    expect(within(money).getByText("EUR")).toBeTruthy();
    // The three figures are shown, never added together.
    expect(within(money).getAllByRole("listitem").length).toBe(1);
    expect(within(money).queryByText(strings.sitesFunnelCurrencies)).toBeNull();
  });

  test("two currencies are two lines and the screen says there is no total", async () => {
    const totals = report().totals;
    mocks.attribution.mockResolvedValue(
      report({
        totals: {
          ...totals,
          money: [
            ...totals.money,
            { currency: "GBP", openCents: 90_000, wonCents: 0, invoicedCents: 0 },
          ],
        },
      }),
    );
    renderFunnel();
    await screen.findByText(strings.sitesFunnelChain);
    const money = panel(strings.sitesFunnelMoney);
    expect(within(money).getAllByRole("listitem").length).toBe(2);
    expect(within(money).getByText(strings.sitesFunnelCurrencies)).toBeTruthy();
  });

  test("Billing switched off hides the invoice figures and says so", async () => {
    const totals = report().totals;
    mocks.attribution.mockResolvedValue(
      report({
        billingVisible: false,
        totals: {
          ...totals,
          invoices: null,
          money: [{ currency: "EUR", openCents: 250_000, wonCents: 180_000, invoicedCents: null }],
        },
        sources: report().sources.map((source) => ({
          ...source,
          invoices: null,
          money: source.money.map((line) => ({ ...line, invoicedCents: null })),
        })),
      }),
    );
    renderFunnel();
    await screen.findByText(strings.sitesFunnelChain);

    // "Not yours to see" is not "nothing was invoiced": the step is dropped
    // rather than drawn as a zero, and the reason is written down.
    const chain = panel(strings.sitesFunnelChain);
    expect(within(chain).getAllByRole("listitem").length).toBe(5);
    expect(within(chain).queryByText(strings.sitesFunnelStageInvoices)).toBeNull();
    expect(screen.getByText(strings.sitesFunnelBillingOff)).toBeTruthy();
    expect(within(panel(strings.sitesFunnelMoney)).getByText(strings.sitesFunnelHidden)).toBeTruthy();
  });

  test("the per-form table names a deleted form and refuses to be an addend", async () => {
    renderFunnel();
    await screen.findByText(strings.sitesFunnelChain);
    const sources = panel(strings.sitesFunnelSources);
    expect(within(sources).getByText("Contact")).toBeTruthy();
    expect(within(sources).getByText(strings.sitesFunnelDeletedSource)).toBeTruthy();
    expect(within(sources).getByText(strings.sitesFunnelDealsSummary(3, 2, 1))).toBeTruthy();
    expect(within(sources).getByText(strings.sitesFunnelSumNote)).toBeTruthy();
  });

  test("a refusal is explained in the server's own words, not as a failure", async () => {
    mocks.attribution.mockRejectedValue(
      new SitesError(403, "alo CRM is switched off for this account"),
    );
    renderFunnel();
    await screen.findByText(strings.sitesFunnelDeniedTitle);
    expect(screen.getByText("alo CRM is switched off for this account")).toBeTruthy();
    expect(screen.getByText(strings.sitesFunnelDeniedWay)).toBeTruthy();
    // A refusal is not a red banner, and it does not leave an empty funnel
    // behind it.
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(strings.sitesFunnelChain)).toBeNull();
  });

  test("a website with no contact form is taught the one next step", async () => {
    mocks.attribution.mockResolvedValue(report({ sources: [] }));
    renderFunnel();
    await screen.findByText(strings.sitesFunnelNoSourcesTitle);
    expect(screen.getByText(strings.sitesFunnelNoSourcesBody)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesOpenPages })).toBeTruthy();
  });

  test("a failure that is not a refusal is shown, never swallowed", async () => {
    mocks.attribution.mockRejectedValue(new Error("nope"));
    renderFunnel();
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe(strings.sitesFunnelLoadFailed);
    });
  });
});

describe("handing an enquiry to sales", () => {
  test("asks only for what a person decides, and retypes nothing", async () => {
    renderInbox();
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesHandoffSubmit }));

    const dialog = await screen.findByRole("dialog");
    // The enquirer travels with the handoff: shown as fact, never as a field.
    expect(within(dialog).getByText("Ada Lovelace")).toBeTruthy();
    expect(within(dialog).getByText("ada@visitor.example")).toBeTruthy();
    expect(within(dialog).getByText(strings.sitesHandoffCarried)).toBeTruthy();

    // The card is already named, and the column defaults to one still in play
    // rather than to "Won" just because it comes first in the list.
    await waitFor(() => {
      expect(
        (within(dialog).getByLabelText(strings.sitesHandoffColumn) as HTMLSelectElement).value,
      ).toBe("stage-new");
    });
    expect(
      (within(dialog).getByLabelText(strings.sitesHandoffCardTitle) as HTMLInputElement).value,
    ).toBe(strings.sitesHandoffTitleFor("Ada Lovelace"));

    fireEvent.change(within(dialog).getByLabelText(strings.sitesHandoffValue), {
      target: { value: "2500,50" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.sitesHandoffSubmit }));

    await waitFor(() => {
      expect(mocks.createSiteLead).toHaveBeenCalledWith("site-1", "sub-1", {
        pipelineId: "board-1",
        stageId: "stage-new",
        title: strings.sitesHandoffTitleFor("Ada Lovelace"),
        companyName: "",
        // Money on the wire is integer cents, read from the decimal the user
        // typed in their own convention.
        valueCents: 250_050,
        currency: "",
        source: "",
      });
    });
    // The result is shown where the decision was made.
    expect(await screen.findByText(strings.sitesInSales)).toBeTruthy();
    expect(screen.getByText("Website enquiry — Ada Lovelace")).toBeTruthy();
  });

  test("an enquiry already handed over offers the way back, not a second card", async () => {
    mocks.siteLeads.mockResolvedValue([link({ deal: { ...link().deal, state: "won" } })]);
    renderInbox();
    await screen.findByText(strings.sitesInSales);
    expect(screen.queryByRole("button", { name: strings.sitesHandoffSubmit })).toBeNull();
    expect(
      screen.getByText(
        strings.sitesLeadStanding(strings.sitesLeadWon, funnelMoney(250_000, "EUR")),
      ),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: strings.sitesUnlinkLead }));
    await waitFor(() => {
      expect(mocks.deleteSiteLead).toHaveBeenCalledWith("site-1", "link-1");
    });
    // Unlinking gives the handoff back, and never claims to have changed the
    // opportunity itself.
    expect(await screen.findByRole("button", { name: strings.sitesHandoffSubmit })).toBeTruthy();
  });

  test("a failed unlink puts the link back and says what happened", async () => {
    mocks.siteLeads.mockResolvedValue([link()]);
    mocks.deleteSiteLead.mockRejectedValue(new SitesError(500, null));
    renderInbox();
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesUnlinkLead }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe(strings.sitesUnlinkLeadFailed);
    });
    expect(screen.getByText(strings.sitesInSales)).toBeTruthy();
  });

  test("a reader who may not see CRM keeps a working inbox", async () => {
    mocks.siteLeads.mockRejectedValue(new SitesError(403, "not yours"));
    renderInbox();
    await screen.findByText("Do you deliver to Ghent?");
    // No handoff, no chip, and above all no error: the refusal is the answer,
    // and the enquiry itself still reads exactly as it did.
    expect(screen.queryByRole("button", { name: strings.sitesHandoffSubmit })).toBeNull();
    expect(screen.queryByText(strings.sitesInSales)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getAllByText("Ada Lovelace").length).toBeGreaterThan(0);
  });

  test("CRM refusing the boards is stated inside the dialog, not as an empty list", async () => {
    mocks.crmBoards.mockRejectedValue(new SitesError(403, "alo CRM is switched off for this account"));
    renderInbox();
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesHandoffSubmit }));
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText("alo CRM is switched off for this account"),
    ).toBeTruthy();
    expect(within(dialog).queryByLabelText(strings.sitesHandoffBoard)).toBeNull();
    const submit = within(dialog).getByRole("button", { name: strings.sitesHandoffSubmit });
    expect((submit as HTMLButtonElement).disabled).toBe(true);
  });

  test("the server's refusal of a handoff is shown verbatim", async () => {
    mocks.createSiteLead.mockRejectedValue(
      new SitesError(422, "this enquiry has already been handed to an opportunity"),
    );
    renderInbox();
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesHandoffSubmit }));
    const dialog = await screen.findByRole("dialog");
    // Both selects have to have answered before the handoff can be made —
    // clicking earlier is the user finding a disabled button, not a bug.
    await waitFor(() => {
      expect(
        (within(dialog).getByLabelText(strings.sitesHandoffBoard) as HTMLSelectElement).value,
      ).toBe("board-1");
      expect(
        (within(dialog).getByLabelText(strings.sitesHandoffColumn) as HTMLSelectElement).value,
      ).toBe("stage-new");
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.sitesHandoffSubmit }));
    await waitFor(() => {
      expect(
        within(dialog).getByText("this enquiry has already been handed to an opportunity"),
      ).toBeTruthy();
    });
  });
});
