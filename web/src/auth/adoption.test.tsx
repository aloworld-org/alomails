// What the second-factor screen gained by adopting `ds/` (D2.05).
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { strings } from "../i18n";
import { TwoFactorScreen } from "./TwoFactorScreen";

afterEach(cleanup);

function open() {
  return render(
    <TwoFactorScreen
      onVerify={() => undefined}
      onBack={() => undefined}
      error={null}
      submitting={false}
    />,
  );
}

describe("the sign-in card is still the form you submit", () => {
  test("the card is the form itself, not a box drawn around one", () => {
    // The risk of adopting `ds/Card` here: wrapping the `<form>` in a card
    // `<div>` would put the padding and the border on something that is not
    // the thing you submit — which is why `Card` took an `as` prop instead.
    //
    // The claim is unchanged; only the way a card can be recognised is. Until
    // D1.52 `Card` carried a hashed `.card` class from its stylesheet, and this
    // asked for it by name. The stylesheet is gone and the surface is Tailwind
    // utilities, so the check is on the surface itself — the ground, the border
    // and the `lg` padding this screen asks for are on the `<form>`, and there
    // is no other element drawing them.
    const { container } = open();
    const form = container.querySelector("form");
    expect(form).not.toBeNull();
    for (const surface of ["bg-surface", "border-subtle", "p-8"]) {
      expect(form!.className).toContain(surface);
      expect(
        [...container.querySelectorAll("*")].filter(
          // `getAttribute`, not `.className`: on an SVG that property is an
          // `SVGAnimatedString` and stringifies to its type name, so a mark
          // inside the card would silently never be checked.
          (el) =>
            el !== form && (el.getAttribute("class") ?? "").includes(surface),
        ),
      ).toEqual([]);
    }
    expect(form!.querySelector("button[type=submit]")).not.toBeNull();
  });

  test("the lock emblem says nothing, out loud", () => {
    // Before: `<div class="badge">` holding a 64px lock. It is decoration over
    // a heading that already says what the screen is, and it was in the
    // reading order as an unnamed group.
    const { container } = open();
    const mark = container.querySelector('[aria-hidden="true"]');
    expect(mark).not.toBeNull();
    expect(mark!.querySelector("svg")).not.toBeNull();
  });
});

describe("the recovery-code field", () => {
  test("it is still named, and typing in it still reaches the caller", () => {
    let submitted: string | null = null;
    render(
      <TwoFactorScreen
        onVerify={(code) => (submitted = code)}
        onBack={() => undefined}
        error={null}
        submitting={false}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.useRecoveryCode }),
    );
    const box = screen.getByLabelText(strings.recoveryCodeLabel);
    fireEvent.change(box, { target: { value: "abcd-efgh" } });
    fireEvent.click(screen.getByRole("button", { name: strings.verify }));
    expect(submitted).toBe("abcd-efgh");
  });

  test("switching back to the authenticator clears what was typed", () => {
    // Behaviour that predates the migration and would have been quiet to
    // break: a recovery code left in state would be submitted as a TOTP.
    render(
      <TwoFactorScreen
        onVerify={() => undefined}
        onBack={() => undefined}
        error={null}
        submitting={false}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.useRecoveryCode }),
    );
    fireEvent.change(screen.getByLabelText(strings.recoveryCodeLabel), {
      target: { value: "abcd-efgh" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.useAuthenticator }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.useRecoveryCode }),
    );
    expect(
      (screen.getByLabelText(strings.recoveryCodeLabel) as HTMLInputElement)
        .value,
    ).toBe("");
  });

  test("submitting is refused while nothing has been typed", () => {
    open();
    const verify = screen.getByRole("button", { name: strings.verify });
    expect((verify as HTMLButtonElement).disabled).toBe(true);
  });
});
