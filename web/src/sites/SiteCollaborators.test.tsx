import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SiteCollaborators } from "./SiteCollaborators";
import { SiteInvitationView } from "./SiteInvitationView";

const mocks = vi.hoisted(() => ({
  collaborators: vi.fn(),
  inviteCollaborator: vi.fn(),
  revokeCollaborator: vi.fn(),
  siteInvitation: vi.fn(),
  acceptSiteInvitation: vi.fn(),
  writeText: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return {
    ...original,
    useSitesApi: () => mocks,
    siteInvitation: mocks.siteInvitation,
    acceptSiteInvitation: mocks.acceptSiteInvitation,
  };
});

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: mocks.writeText },
  });
  mocks.writeText.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("site collaborators", () => {
  test("invites, copies, revokes, and restores from the visible surface", async () => {
    const collaborator = {
      id: "editor-1",
      email: "editor@example.test",
      status: "pending" as const,
    };
    mocks.collaborators
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([collaborator])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([collaborator]);
    mocks.inviteCollaborator.mockResolvedValue({
      collaborator,
      inviteUrl: "http://localhost:5173/sites/invite/one-time-token",
    });
    mocks.revokeCollaborator.mockResolvedValue(undefined);

    render(<SiteCollaborators siteId="site-1" />);
    expect(await screen.findByText(strings.sitesNoCollaborators)).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.sitesCollaboratorEmail), {
      target: { value: collaborator.email },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesInviteCollaborator }));

    const copy = await screen.findByRole("button", {
      name: strings.sitesCopyCollaboratorLink,
    });
    expect(mocks.inviteCollaborator).toHaveBeenCalledWith("site-1", collaborator.email);
    fireEvent.click(copy);
    await waitFor(() =>
      expect(mocks.writeText).toHaveBeenCalledWith(
        "http://localhost:5173/sites/invite/one-time-token",
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: strings.sitesRevokeCollaborator }));
    expect(await screen.findByText(strings.sitesCollaboratorRevoked(collaborator.email))).toBeTruthy();
    expect(mocks.revokeCollaborator).toHaveBeenCalledWith("site-1", collaborator.id);

    fireEvent.click(screen.getByRole("button", { name: strings.sitesUndoCollaboratorRevoke }));
    await waitFor(() => expect(mocks.inviteCollaborator).toHaveBeenCalledTimes(2));
  });
});

describe("the public collaborator setup", () => {
  test("shows the exact site and creates the restricted sign-in", async () => {
    const invitation = {
      email: "editor@example.test",
      siteName: "Alpha Bakery",
    };
    mocks.siteInvitation.mockResolvedValue(invitation);
    mocks.acceptSiteInvitation.mockResolvedValue({ ...invitation, status: "accepted" });

    render(
      <MemoryRouter initialEntries={["/sites/invite/one-time-token"]}>
        <Routes>
          <Route path="/sites/invite/:token" element={<SiteInvitationView />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(await screen.findByText(strings.sitesInvitationSubtitle(invitation.siteName))).toBeTruthy();
    fireEvent.change(screen.getByLabelText(strings.sitesInvitationPassword), {
      target: { value: "a-private-password" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesInvitationConfirmPassword), {
      target: { value: "a-private-password" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesInvitationAccept }));

    expect(await screen.findByText(strings.sitesInvitationDone)).toBeTruthy();
    expect(mocks.acceptSiteInvitation).toHaveBeenCalledWith(
      "one-time-token",
      "a-private-password",
    );
    expect(screen.getByRole("link", { name: strings.sitesInvitationSignIn })).toBeTruthy();
  });
});
