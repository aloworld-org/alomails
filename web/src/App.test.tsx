import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { App } from "./App";
import { strings } from "./i18n/strings";

// With no stored session, the app boots to the sign-in screen (RequireAuth
// sends an unauthenticated visitor to /login). We assert the login title
// appears — proving the router, auth bootstrap, and shell wiring compose.
test("an unauthenticated visit lands on the sign-in screen", async () => {
  render(<App />);
  const heading = await screen.findByRole("heading", { name: strings.signInHeading });
  expect(heading).toBeTruthy();
});
