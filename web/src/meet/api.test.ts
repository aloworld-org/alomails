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

  test("in-call messages are persisted with their private recipient", async () => {
    const { api, fetched } = client(200, { id: "msg-1", body: "hello", recipient: "user-2" });
    await expect(api.postMessage("m-1", "hello", "user-2")).resolves.toMatchObject({ id: "msg-1" });
    expect(fetched).toHaveBeenCalledWith(
      expect.stringContaining("/api/meet/m-1/messages"),
      expect.objectContaining({ method: "POST", body: JSON.stringify({ body: "hello", recipient: "user-2" }) }),
    );
  });

  test("meeting attachments are uploaded as their real media type", async () => {
    const { api, fetched } = client(200, { id: "file-1", name: "plan.pdf" });
    const file = new File(["pdf"], "plan.pdf", { type: "application/pdf" });
    await expect(api.uploadAttachment("m-1", "msg-1", file)).resolves.toMatchObject({ id: "file-1" });
    expect(fetched).toHaveBeenCalledWith(
      expect.stringContaining("/api/meet/m-1/messages/msg-1/attachments?name=plan.pdf"),
      expect.objectContaining({ method: "POST", body: file, headers: { "content-type": "application/pdf" } }),
    );
  });

  test("meeting reactions are persisted against the durable message", async () => {
    const { api, fetched } = client(200);
    await expect(api.react("m-1", "msg-1", "👍")).resolves.toBeUndefined();
    expect(fetched).toHaveBeenCalledWith(
      expect.stringContaining("/api/meet/m-1/messages/msg-1/reactions"),
      expect.objectContaining({ method: "POST", body: JSON.stringify({ emoji: "👍" }) }),
    );
  });

  test("final caption segments are persisted as meeting transcript", async () => {
    const { api, fetched } = client(200, { id: "seg-1", text: "A decision", final: true });
    await expect(api.putTranscriptSegment("m-1", { id: "seg-1", text: "A decision", final: true })).resolves.toMatchObject({ id: "seg-1", final: true });
    expect(fetched).toHaveBeenCalledWith(
      expect.stringContaining("/api/meet/m-1/transcript"),
      expect.objectContaining({ method: "POST", body: JSON.stringify({ id: "seg-1", text: "A decision", final: true }) }),
    );
  });

  test("host moderation stays behind alo's authenticated meeting API", async () => {
    const { api, fetched } = client(200);
    await expect(api.moderate("m-1", "mute", "user-2", "TR_audio")).resolves.toBeUndefined();
    expect(fetched).toHaveBeenCalledWith(
      expect.stringContaining("/api/meet/m-1/moderate"),
      expect.objectContaining({ method: "POST", body: JSON.stringify({ action: "mute", participant: "user-2", trackSid: "TR_audio" }) }),
    );
  });

  test("recording consent and control stay behind alo's meeting API", async () => {
    const recording = { id: "rec-1", status: "pending", consents: [] };
    const requested = client(200, recording);
    await expect(requested.api.requestRecording("m-1")).resolves.toMatchObject({ id: "rec-1" });
    expect(requested.fetched).toHaveBeenCalledWith(expect.stringContaining("/api/meet/m-1/recordings"), expect.objectContaining({ method: "POST" }));

    const consented = client(200, recording);
    await consented.api.consentRecording("m-1", "rec-1");
    expect(consented.fetched).toHaveBeenCalledWith(expect.stringContaining("/api/meet/m-1/recordings/rec-1/consent"), expect.objectContaining({ method: "POST" }));

    const started = client(200, { ...recording, status: "recording" });
    await started.api.startRecording("m-1", "rec-1");
    expect(started.fetched).toHaveBeenCalledWith(expect.stringContaining("/api/meet/m-1/recordings/rec-1/start"), expect.objectContaining({ method: "POST" }));

    const stopped = client(200, { ...recording, status: "completed" });
    await stopped.api.stopRecording("m-1", "rec-1");
    expect(stopped.fetched).toHaveBeenCalledWith(expect.stringContaining("/api/meet/m-1/recordings/rec-1/stop"), expect.objectContaining({ method: "POST" }));
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
