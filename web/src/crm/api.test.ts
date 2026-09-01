import { describe, expect, test, vi } from "vitest";

import { CrmApi } from "./api";

describe("Mail opportunity contract", () => {
  test("sends the source thread on the ordinary deal create request", async () => {
    const authorizedFetch = vi.fn(async (input: string, init?: RequestInit) => {
      void input;
      void init;
      return new Response(
        JSON.stringify({
          deal: { id: "deal-1" },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    });
    const api = new CrmApi(authorizedFetch);

    await api.createDealFromMail({
      pipelineId: "pipeline-1",
      stageId: "stage-1",
      threadId: "thread-1",
      title: "New website",
    });

    const [url, init] = authorizedFetch.mock.calls[0] ?? [];
    expect(url).toContain("/api/crm/deals");
    expect(init?.method).toBe("POST");
    expect(JSON.parse(String(init?.body))).toEqual({
      pipelineId: "pipeline-1",
      stageId: "stage-1",
      threadId: "thread-1",
      title: "New website",
    });
  });
});
