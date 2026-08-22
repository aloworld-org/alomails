import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AuthProvider, useAuth } from "./AuthProvider";
import { clearSession, getRefreshToken, setRefreshToken } from "./session";

vi.mock("./oidcClient", () => ({
  login: vi.fn(),
  refresh: vi.fn().mockResolvedValue(null),
  revoke: vi.fn().mockResolvedValue(undefined),
}));

function Status() {
  return <span>{useAuth().status}</span>;
}

describe("AuthProvider session bootstrap", () => {
  beforeEach(() => clearSession());
  afterEach(() => {
    cleanup();
    clearSession();
  });

  test("discards a rejected refresh token instead of retrying it after every reload", async () => {
    setRefreshToken("dead-refresh-token");

    render(
      <AuthProvider>
        <Status />
      </AuthProvider>,
    );

    await waitFor(() => expect(screen.getByText("anonymous")).toBeTruthy());
    expect(getRefreshToken()).toBeNull();
  });
});
