// What the domains screen must keep doing (S2.15c3):
//
//   * a deployment that sells no domains says so in the server's own words and
//     still offers the path that always works — connecting a domain you own;
//   * a TXT proof is shown exactly as the server composed it, and a check that
//     has not found it yet is a sentence about DNS, not a red failure;
//   * a search never states one price without the other, and never prices what
//     nobody can buy;
//   * buying sends no price at all, and approving echoes the exact numbers the
//     screen had up;
//   * a purchase past payment offers no call-off button, because no route
//     performs one, and a failed one reads the registrar's own sentence.
//
// Same harness as Booking.test.tsx: the real API client and the real views
// run, and only the network is faked, so the URLs and bodies asserted here are
// the ones the wire-verified S2.15c1/c2 routes take.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import type { SiteDomain, SiteDomainPurchase } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
}

const calls: Call[] = [];
let replies: Reply[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((reply) => reply.match(url, method));
  const answer =
    index === -1 ? { status: 200, body: {} } : (replies.splice(index, 1)[0] as Reply);
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

const SITE = {
  id: "site-1",
  name: "Axon",
  subdomain: "axon",
  status: "live",
  defaultLocale: "en",
  enabledLocales: ["en"],
  publish: null,
  canManageCollaborators: true,
  theme: {},
};

const PENDING: SiteDomain = {
  domain: "axon.example",
  status: "pending",
  verifiedAt: null,
  verifyRecord: {
    name: "_alo-site.axon.example",
    type: "TXT",
    value: "alo-site-verification=tok-123",
  },
  createdAt: "2026-08-13T08:00:00Z",
  updatedAt: "2026-08-13T08:00:00Z",
};

const SERVING: SiteDomain = {
  ...PENDING,
  status: "live",
  verifiedAt: "2026-08-13T09:00:00Z",
};

/** The catalog of a deployment that does sell domains, with the fixture
 *  reseller behind it. */
const CATALOG = {
  registrar: {
    name: "alo fixture registrar",
    country: "nl",
    environment: "fixture",
    spendsMoney: false,
  },
  currency: "EUR",
  buyable: true,
  endings: [
    {
      tld: "com",
      registerCents: 1208,
      renewCents: 1208,
      transferCents: 1208,
      minYears: 1,
      maxYears: 3,
      requirement: { kind: "none" },
    },
    {
      tld: "eu",
      registerCents: 750,
      renewCents: 750,
      transferCents: 750,
      minYears: 1,
      maxYears: 2,
      requirement: { kind: "eea_presence" },
    },
  ],
};

const SEARCH = {
  label: "acme",
  currency: "EUR",
  buyable: true,
  offers: [
    {
      domain: "acme.com",
      availability: "available",
      quote: {
        domain: "acme.com",
        termYears: 1,
        currency: "EUR",
        firstTermCents: 1208,
        renewalCentsPerYear: 1208,
        premium: false,
      },
    },
    { domain: "acme.eu", availability: "taken", quote: null },
  ],
};

const QUOTED: SiteDomainPurchase = {
  id: "purchase-1",
  site: "site-1",
  kind: "registration",
  domain: "acme.com",
  tld: "com",
  state: "quoted",
  moneyMoved: false,
  open: true,
  termYears: 1,
  currency: "EUR",
  firstTermCents: 1208,
  renewalCentsPerYear: 1208,
  premium: false,
  autoRenew: true,
  nameservers: ["ns1.alosites.com", "ns2.alosites.com"],
  requestKey: "key-1",
  approvedAt: null,
  approvedBy: null,
  paymentReference: null,
  paidAt: null,
  attempts: 0,
  providerReference: null,
  registeredAt: null,
  expiresAt: null,
  lifecycle: null,
  configuredAt: null,
  failure: null,
  createdAt: "2026-08-13T08:00:00Z",
  updatedAt: "2026-08-13T08:00:00Z",
};

const PAID: SiteDomainPurchase = {
  ...QUOTED,
  id: "purchase-2",
  state: "paid",
  moneyMoved: true,
  approvedAt: "2026-08-13T08:10:00Z",
  approvedBy: "user-1",
  paymentReference: "pi_wire_1",
  paidAt: "2026-08-13T08:20:00Z",
};

const FAILED: SiteDomainPurchase = {
  ...QUOTED,
  id: "purchase-3",
  state: "failed",
  moneyMoved: false,
  open: false,
  failure:
    "That domain was registered by somebody else while the payment was in flight; the charge will be refunded.",
};

function get(match: (url: string) => boolean, body: unknown, status = 200): Reply {
  return { match: (url, method) => method === "GET" && match(url), status, body };
}

function post(match: (url: string) => boolean, body: unknown, status = 200): Reply {
  return { match: (url, method) => method === "POST" && match(url), status, body };
}

/** The four reads the screen makes on arrival. `catalog` is what the buy half
 *  gets: either an offer list or the deployment's own refusal. */
function screenReplies(options: {
  domains?: SiteDomain[];
  purchases?: SiteDomainPurchase[];
  catalog?: Reply;
}): Reply[] {
  return [
    get((url) => url.endsWith("/sites/site-1"), SITE),
    get((url) => url.endsWith("/sites/site-1/domain-purchases"), {
      purchases: options.purchases ?? [],
    }),
    get((url) => url.endsWith("/sites/config"), { domain: "alosites.com" }),
    get((url) => url.endsWith("/sites/site-1/domains"), {
      domains: options.domains ?? [],
    }),
    options.catalog ?? UNCONFIGURED,
  ];
}

/** What production answers today: no reseller wired, and a sentence that names
 *  the way on. */
const UNCONFIGURED: Reply = get(
  (url) => url.endsWith("/sites/domain-catalog"),
  {
    detail:
      "Buying domain names is not configured on this alo deployment. You can still connect a domain you already own.",
    reason: "unconfigured",
  },
  503,
);

const SELLING: Reply = get((url) => url.endsWith("/sites/domain-catalog"), CATALOG);

function ui() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/domains"]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

function lastWrite(): Call | undefined {
  return calls.filter((call) => call.method !== "GET").at(-1);
}

/** Types a name into the search box and waits for the debounced request. */
async function search(name: string) {
  fireEvent.change(screen.getByLabelText(strings.sitesDomainSearchLabel), {
    target: { value: name },
  });
  await waitFor(() =>
    expect(calls.some((call) => call.url.includes("/sites/domain-search"))).toBe(true),
  );
}

/** Fills the registrant form with an address a registry would accept. */
function fillRegistrant() {
  const type = (label: string, value: string) => {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  };
  type(strings.sitesDomainRegistrantName, "Iris de Vries");
  type(strings.sitesDomainRegistrantEmail, "iris@axon.example");
  type(strings.sitesDomainRegistrantStreet, "Keizersgracht 1");
  type(strings.sitesDomainRegistrantPostalCode, "1015 CJ");
  type(strings.sitesDomainRegistrantCity, "Amsterdam");
  type(strings.sitesDomainRegistrantCountry, "NL");
  type(strings.sitesDomainRegistrantPhone, "+31201234567");
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("a deployment that sells no domains", () => {
  test("says so in the server's own words and still offers the path that works", async () => {
    replies = screenReplies({});

    ui();

    expect(await screen.findByText(strings.sitesDomainUnconfiguredTitle)).toBeTruthy();
    // The server's sentence, verbatim — it is the one that names the way on.
    expect(
      screen.getByText(/You can still connect a domain you already own/),
    ).toBeTruthy();
    // Connecting a domain is still fully available…
    expect(screen.getByRole("button", { name: strings.sitesDomainAdd })).toBeTruthy();
    // …and nothing offers to sell what this deployment cannot register.
    expect(screen.queryByLabelText(strings.sitesDomainSearchLabel)).toBeNull();
  });
});

describe("a restricted site editor", () => {
  test("keeps the domains they may manage, and is never shown a money door", async () => {
    replies = [
      get((url) => url.endsWith("/sites/site-1"), {
        ...SITE,
        canManageCollaborators: false,
      }),
      get((url) => url.endsWith("/sites/config"), { domain: "alosites.com" }),
      get((url) => url.endsWith("/sites/site-1/domains"), { domains: [SERVING] }),
    ];

    ui();

    expect(await screen.findByText(strings.sitesDomainOwnerOnly)).toBeTruthy();
    expect(await screen.findByText("axon.example")).toBeTruthy();
    expect(screen.queryByLabelText(strings.sitesDomainSearchLabel)).toBeNull();
    // Not merely hidden: the panels that would 403 are never asked for.
    expect(calls.some((call) => call.url.includes("/domain-purchases"))).toBe(false);
    expect(calls.some((call) => call.url.includes("/domain-catalog"))).toBe(false);
  });
});

describe("connecting a domain you already own", () => {
  test("shows the exact record the server composed, all three fields of it", async () => {
    replies = screenReplies({ domains: [PENDING] });

    ui();

    expect(await screen.findByText(strings.sitesDomainRecordTitle)).toBeTruthy();
    expect(screen.getByText("_alo-site.axon.example")).toBeTruthy();
    expect(screen.getByText("TXT")).toBeTruthy();
    expect(screen.getByText("alo-site-verification=tok-123")).toBeTruthy();
  });

  test("a check that has not found the record yet is a sentence about DNS, not a failure", async () => {
    replies = [
      ...screenReplies({ domains: [PENDING] }),
      // The route answers the unchanged claim: the record has not travelled.
      post((url) => url.endsWith("/domains/axon.example/verify"), PENDING),
    ];

    ui();

    fireEvent.click(await screen.findByRole("button", { name: strings.sitesDomainCheck }));

    expect(await screen.findByText(strings.sitesDomainNotYet)).toBeTruthy();
    expect(lastWrite()?.url).toContain("/sites/site-1/domains/axon.example/verify");
    // Still pending, so the record stays on screen to be copied again.
    expect(screen.getByText(strings.sitesDomainRecordTitle)).toBeTruthy();
  });

  test("a verified domain is told the last step, with this website's own address in it", async () => {
    replies = screenReplies({ domains: [SERVING] });

    ui();

    expect(
      await screen.findByText(strings.sitesDomainPointHint("axon.alosites.com")),
    ).toBeTruthy();
  });

  test("the server's refusal is what a person reads", async () => {
    replies = [
      ...screenReplies({}),
      post((url) => url.endsWith("/sites/site-1/domains"), {
        detail: "a domain is a name plus its ending, such as acme.com",
      }, 422),
    ];

    ui();

    fireEvent.change(await screen.findByLabelText(strings.sitesDomainAddress), {
      target: { value: "acme" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesDomainAdd }));

    expect(
      await screen.findByText("a domain is a name plus its ending, such as acme.com"),
    ).toBeTruthy();
  });
});

describe("the buy box", () => {
  test("prices nothing that cannot be bought, and never states one price without the other", async () => {
    replies = [
      ...screenReplies({ catalog: SELLING }),
      get((url) => url.includes("/sites/domain-search"), SEARCH),
    ];

    ui();

    await screen.findByLabelText(strings.sitesDomainSearchLabel);
    await search("acme");

    // The name is searched as typed; the server understands the rest.
    expect(
      calls.find((call) => call.url.includes("/sites/domain-search"))?.url,
    ).toContain("q=acme");
    // The free one carries today's price AND the renewal price, in one line.
    expect(
      await screen.findByText(strings.sitesDomainPriceLine("€12.08", "€12.08")),
    ).toBeTruthy();
    // The taken one is listed — so nobody wonders — and carries no price.
    expect(screen.getByText("acme.eu")).toBeTruthy();
    expect(screen.getByText(strings.sitesDomainTaken)).toBeTruthy();
    expect(screen.getAllByText(/12\.08/)).toHaveLength(1);
    // A fixture reseller is badged rather than hidden.
    expect(
      screen.getByText(strings.sitesDomainTestRegistrar("alo fixture registrar")),
    ).toBeTruthy();
  });

  test("buying sends no price, and approving echoes the exact numbers on screen", async () => {
    replies = [
      ...screenReplies({ catalog: SELLING }),
      get((url) => url.includes("/sites/domain-search"), SEARCH),
      post((url) => url.endsWith("/sites/site-1/domain-purchases"), QUOTED),
      post((url) => url.endsWith("/purchase-1/approve"), {
        ...QUOTED,
        state: "approved",
        approvedAt: "2026-08-13T08:10:00Z",
        approvedBy: "user-1",
      }),
    ];

    ui();

    await screen.findByLabelText(strings.sitesDomainSearchLabel);
    await search("acme");
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesDomainChoose }),
    );

    fillRegistrant();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesDomainSeePrice }));

    await waitFor(() => expect(lastWrite()?.url).toContain("/domain-purchases"));
    const created = lastWrite()?.body as Record<string, unknown>;
    // No price travelled through the browser — not the total, not the renewal.
    expect(Object.keys(created)).toEqual([
      "domain",
      "years",
      "autoRenew",
      "requestKey",
      "registrant",
    ]);
    expect(created.domain).toBe("acme.com");
    expect(created.years).toBe(1);
    expect(created.autoRenew).toBe(true);
    expect(String(created.requestKey).length).toBeGreaterThanOrEqual(8);
    expect(created.registrant).toEqual({
      name: "Iris de Vries",
      organisation: null,
      email: "iris@axon.example",
      street: "Keizersgracht 1",
      postalCode: "1015 CJ",
      city: "Amsterdam",
      // A country typed in capitals is normalized, not refused.
      country: "nl",
      phone: "+31201234567",
    });

    // Step two states both halves of the price before anything is agreed.
    expect(await screen.findByText(strings.sitesDomainApproveTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesDomainQuoteRenewal)).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesDomainApproveAction("€12.08") }),
    );

    await waitFor(() => expect(lastWrite()?.url).toContain("/purchase-1/approve"));
    expect(lastWrite()?.body).toEqual({
      agreed: {
        domain: "acme.com",
        termYears: 1,
        currency: "EUR",
        firstTermCents: 1208,
        renewalCentsPerYear: 1208,
        premium: false,
      },
    });
  });

  test("a refused registrant is explained in the server's words, with the form still filled in", async () => {
    replies = [
      ...screenReplies({ catalog: SELLING }),
      get((url) => url.includes("/sites/domain-search"), SEARCH),
      post((url) => url.endsWith("/sites/site-1/domain-purchases"), {
        detail:
          "the registrant telephone must be in international form, such as +31201234567",
      }, 422),
    ];

    ui();

    await screen.findByLabelText(strings.sitesDomainSearchLabel);
    await search("acme");
    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesDomainChoose }),
    );
    fillRegistrant();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesDomainSeePrice }));

    expect(
      await screen.findByText(
        "the registrant telephone must be in international form, such as +31201234567",
      ),
    ).toBeTruthy();
    // Nothing typed was thrown away.
    expect(
      (screen.getByLabelText(strings.sitesDomainRegistrantName) as HTMLInputElement)
        .value,
    ).toBe("Iris de Vries");
  });
});

describe("the record of what was bought", () => {
  test("a quoted purchase can still be approved from its own row", async () => {
    replies = [
      ...screenReplies({ purchases: [QUOTED] }),
      post((url) => url.endsWith("/purchase-1/approve"), {
        ...QUOTED,
        state: "approved",
      }),
    ];

    ui();

    expect(await screen.findByText(strings.sitesDomainStepQuoted)).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesDomainApproveAction("€12.08") }),
    );

    await waitFor(() => expect(lastWrite()?.url).toContain("/purchase-1/approve"));
    expect(lastWrite()?.body).toEqual({
      agreed: {
        domain: "acme.com",
        termYears: 1,
        currency: "EUR",
        firstTermCents: 1208,
        renewalCentsPerYear: 1208,
        premium: false,
      },
    });
    // The row moves on, and stops offering an approval it already has.
    expect(await screen.findByText(strings.sitesDomainStepApproved)).toBeTruthy();
  });

  test("a purchase that has been paid for offers no button that no route performs", async () => {
    replies = screenReplies({ purchases: [PAID] });

    ui();

    expect(await screen.findByText(strings.sitesDomainStepPaid)).toBeTruthy();
    expect(screen.queryByRole("button", { name: strings.sitesDomainCancel })).toBeNull();
    // It is still moving, so the way to watch it is on screen.
    expect(screen.getByRole("button", { name: strings.sitesDomainRefresh })).toBeTruthy();
  });

  test("a purchase that failed reads the registrar's own sentence about the money", async () => {
    replies = screenReplies({ purchases: [FAILED] });

    ui();

    expect(await screen.findByText(FAILED.failure ?? "")).toBeTruthy();
    expect(screen.queryByRole("button", { name: strings.sitesDomainRefresh })).toBeNull();
  });

  test("calling off a purchase asks once, then does it", async () => {
    replies = [
      ...screenReplies({ purchases: [QUOTED] }),
      post((url) => url.endsWith("/purchase-1/cancel"), {
        ...QUOTED,
        state: "cancelled",
        open: false,
      }),
    ];

    ui();

    fireEvent.click(await screen.findByRole("button", { name: strings.sitesDomainCancel }));
    // The first click arms; nothing has been sent.
    expect(lastWrite()).toBeUndefined();
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesDomainCancelConfirm }),
    );

    await waitFor(() => expect(lastWrite()?.url).toContain("/purchase-1/cancel"));
    expect(await screen.findByText(strings.sitesDomainStepCancelled)).toBeTruthy();
  });
});
