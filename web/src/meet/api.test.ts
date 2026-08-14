// The one thing this client must get right: a deployment with no media engine
// is a different situation from a failure, and has to read differently.
import { describe, expect, test, vi } from "vitest";

import { MeetApi, MeetUnavailable } from "./api";

function client(status: number, body: unknown = {}) {
  const fetched = vi.fn(
    async () =>
      new Response(JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      }),
  );
  return { api: new MeetApi(fetched as never), fetched };
}

describe("joining a meeting", () => {
  test("meeting history is loaded from the dedicated ended-meetings route", async () => {
    const { api, fetched } = client(200, { meetings: [{ id: "m-ended", live: false }] });
    await expect(api.history()).resolves.toEqual([{ id: "m-ended", live: false }]);
    expect(fetched).toHaveBeenCalledWith(expect.stringContaining("/api/meet/history"), expect.any(Object));
  });

  test("a deployment with no engine says so, distinctly", async () => {
    // 503 means the meeting is real and attendance was recorded — there is
    // simply nowhere to hold it. Reporting that as a generic failure would
    // send an administrator looking for a bug instead of a setting.
    const { api } = client(503);
    await expect(api.join("m-1")).rejects.toBeInstanceOf(MeetUnavailable);
  });

  test("a real failure is not mistaken for an unconfigured one", async () => {
    const { api } = client(404);
    const failure = await api.join("m-1").catch((e: unknown) => e);
    expect(failure).toBeInstanceOf(Error);
    expect(failure).not.toBeInstanceOf(MeetUnavailable);
  });

  test("a grant carries the engine's url and a token, and nothing else is needed", async () => {
    const { api, fetched } = client(200, {
      meeting: { id: "m-1", live: true },
      url: "wss://meet.example",
      token: "signed",
    });
    const grant = await api.join("m-1");
    expect(grant.url).toBe("wss://meet.example");
    expect(grant.token).toBe("signed");
    // The room name the engine knows is never sent to the browser on its own:
    // it rides inside the token, so it cannot be passed around as a way in.
    expect(JSON.stringify(grant)).not.toContain("room");
    expect(fetched).toHaveBeenCalledOnce();
  });

  test("rooms with nothing running answer with an empty list, not an error", async () => {
    const { api } = client(404);
    await expect(api.liveIn("room-1")).resolves.toEqual([]);
  });
});
