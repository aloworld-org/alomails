import { beforeEach, describe, expect, test, vi } from "vitest";

vi.mock("./pkce", () => ({
  challengeFor: vi.fn().mockResolvedValue("challenge"),
  createState: vi.fn().mockReturnValue("state"),
  createVerifier: vi.fn().mockReturnValue("verifier"),
}));

describe("login failure classification", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  test("an unavailable backend is not reported as a wrong password", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 503 })));
    const { login } = await import("./oidcClient");

    await expect(login("person@alomails.com", "password")).rejects.toEqual(
      expect.objectContaining({ kind: "network" }),
    );
  });

  test("only an authentication refusal is reported as wrong credentials", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        Response.json(
          { error: "access_denied", error_description: "invalid credentials" },
          { status: 401 },
        ),
      ),
    );
    const { login } = await import("./oidcClient");

    await expect(login("person@alomails.com", "wrong")).rejects.toEqual(
      expect.objectContaining({ kind: "bad_credentials" }),
    );
  });
});
